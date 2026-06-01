use hudhook::hooks::opengl3::ImguiOpenGl3Hooks;
use hudhook::imgui::{Condition, Ui, WindowFlags, CollapsingHeader};
use hudhook::{Hudhook, ImguiRenderLoop, eject};
use lazy_static::lazy_static;
use std::fs::OpenOptions;
use std::io::Write;
use winapi::um::utilapiset::Beep;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::ffi::CString;
use std::ptr;
use std::time::Instant;
use winapi::um::winuser::{OpenClipboard, EmptyClipboard, SetClipboardData, CloseClipboard};
use winapi::um::winbase::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use winapi::um::winnt::HANDLE;
use winapi::ctypes::c_void;
use winapi::shared::minwindef::{BOOL, DWORD, TRUE};
use hudhook::windows::Win32::Foundation::HINSTANCE;
use winapi::um::errhandlingapi::AddVectoredExceptionHandler;
use winapi::um::processthreadsapi::GetCurrentProcess;
use winapi::um::handleapi::CloseHandle;
use winapi::um::libloaderapi::GetModuleHandleA;
use winapi::um::libloaderapi::GetModuleFileNameA;
use winapi::um::memoryapi::{ReadProcessMemory, WriteProcessMemory};
use winapi::um::processthreadsapi::{GetThreadContext, OpenThread, SetThreadContext};
use winapi::um::tlhelp32::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32};
use winapi::um::winuser::{GetAsyncKeyState, VK_F2, VK_INSERT, VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN};
use winapi::um::winnt::{
    CONTEXT,
    CONTEXT_DEBUG_REGISTERS,
    EXCEPTION_POINTERS,
};

// constants
const OFFSET: usize = 0x2DDDF2;
const EXCEPTION_SINGLE_STEP: DWORD = 0x80000004;
const EXCEPTION_CONTINUE_EXECUTION: i32 = -1;
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
const MAX_PROPERTIES: usize = 5;
const CAPTURE_TIMEOUT_MS: u128 = 25;

fn log_debug(msg: &str) {
    let paths = [
        GAME_LOG_PATH.lock().unwrap().clone(),
        DLL_LOG_PATH.lock().unwrap().clone(),
    ];

    for path in paths.into_iter().flatten() {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(file, "{}", msg);
        }
    }
}

// global state
lazy_static! {
    static ref TARGET_INST: Mutex<Option<usize>> = Mutex::new(None);
    static ref CAPTURE_MODE: AtomicBool = AtomicBool::new(false);
    static ref BREAKPOINT_SET: AtomicBool = AtomicBool::new(false);
    static ref LAST_PROP_ADDRS: Mutex<[usize; MAX_PROPERTIES]> =
	Mutex::new([0; MAX_PROPERTIES]);
	static ref CAPTURE_COUNT: AtomicU32 = AtomicU32::new(0);
	static ref CAPTURE_SERIAL: AtomicU32 = AtomicU32::new(0);
	static ref GAME_LOG_PATH: Mutex<Option<String>> = Mutex::new(None);
    static ref DLL_LOG_PATH: Mutex<Option<String>> = Mutex::new(None);
	static ref BLOCK_GAME_WRITES: AtomicBool = AtomicBool::new(false);
	static ref LAST_CAPTURE_TIME: Mutex<Option<Instant>> = Mutex::new(None);
}

// get module base address
fn get_module_base() -> Option<usize> {
    unsafe {
        let module = GetModuleHandleA("Stormworks64.exe\0".as_ptr() as *const i8);
        if module.is_null() {
            log_debug("GetModuleHandleA failed - module not found");
            None
        } else {
            let addr = module as usize;
            log_debug(&format!("Module base address: 0x{:X}", addr));
            Some(addr)
        }
    }
}

unsafe fn module_dir_log_path(module: *mut c_void) -> Option<String> {
    let mut buf = [0i8; 260];

    let len = GetModuleFileNameA(
        module as _,
        buf.as_mut_ptr(),
        buf.len() as u32,
    );

    if len == 0 {
        return None;
    }

    let path = std::ffi::CStr::from_ptr(buf.as_ptr())
        .to_string_lossy()
        .into_owned();

    let mut p = std::path::PathBuf::from(path);
    p.pop();
    p.push("swripe.log");

    Some(p.to_string_lossy().into_owned())
}

unsafe fn copy_to_clipboard(text: &str) {
    let c_text = CString::new(text).unwrap();
    let size = c_text.as_bytes_with_nul().len();

    if OpenClipboard(ptr::null_mut()) == 0 {
        log_debug("OpenClipboard failed");
        return;
    }

    EmptyClipboard();

    let h_mem = GlobalAlloc(GMEM_MOVEABLE, size);
    if h_mem.is_null() {
        CloseClipboard();
        log_debug("GlobalAlloc failed");
        return;
    }

    let locked = GlobalLock(h_mem) as *mut u8;
    if locked.is_null() {
        CloseClipboard();
        log_debug("GlobalLock failed");
        return;
    }

    std::ptr::copy_nonoverlapping(
        c_text.as_ptr() as *const u8,
        locked,
        size,
    );

    GlobalUnlock(h_mem);

    if SetClipboardData(1, h_mem as HANDLE).is_null() {
        log_debug("SetClipboardData failed");
    }

    CloseClipboard();
}

// set breakpoint on all threads
unsafe fn set_breakpoint_on_all_threads() {
    log_debug("set_breakpoint_on_all_threads called");
    
    let pid = std::process::id();
    let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    if snapshot as isize == -1 {
        log_debug("CreateToolhelp32Snapshot failed");
        return;
    }

    let mut te: THREADENTRY32 = std::mem::zeroed();
    te.dwSize = std::mem::size_of::<THREADENTRY32>() as DWORD;
    let mut thread_count = 0;

    if Thread32First(snapshot, &mut te) != 0 {
        loop {
            if te.th32OwnerProcessID == pid {
                let thread = OpenThread(0x0010 | 0x0008 | 0x0002 | 0x0004, 0, te.th32ThreadID);
                if !thread.is_null() {
                    let mut ctx: CONTEXT = std::mem::zeroed();
                    ctx.ContextFlags = CONTEXT_DEBUG_REGISTERS;
                    if GetThreadContext(thread, &mut ctx) != 0 {
                        if let Some(addr) = *TARGET_INST.lock().unwrap() {
                            ctx.Dr0 = addr as u64;
                            ctx.Dr7 = 1;
                            SetThreadContext(thread, &ctx);
                            thread_count += 1;
                        }
                    }
                    CloseHandle(thread);
                }
            }
            if Thread32Next(snapshot, &mut te) == 0 {
                break;
            }
        }
    }
    CloseHandle(snapshot);
    log_debug(&format!("Breakpoint set on {} threads", thread_count));
}

// remove breakpoint from all threads
unsafe fn remove_breakpoint_from_all_threads() {
    log_debug("remove_breakpoint_from_all_threads called");
    
    let pid = std::process::id();
    let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    if snapshot as isize == -1 {
        return;
    }

    let mut te: THREADENTRY32 = std::mem::zeroed();
    te.dwSize = std::mem::size_of::<THREADENTRY32>() as DWORD;
    let mut thread_count = 0;

    if Thread32First(snapshot, &mut te) != 0 {
        loop {
            if te.th32OwnerProcessID == pid {
                let thread = OpenThread(0x0010 | 0x0008 | 0x0002 | 0x0004, 0, te.th32ThreadID);
                if !thread.is_null() {
                    let mut ctx: CONTEXT = std::mem::zeroed();
                    ctx.ContextFlags = CONTEXT_DEBUG_REGISTERS;
                    if GetThreadContext(thread, &mut ctx) != 0 {
                        ctx.Dr0 = 0;
                        ctx.Dr7 = 0;
                        SetThreadContext(thread, &ctx);
                        thread_count += 1;
                    }
                    CloseHandle(thread);
                }
            }
            if Thread32Next(snapshot, &mut te) == 0 {
                break;
            }
        }
    }
    CloseHandle(snapshot);
    log_debug(&format!("Breakpoint removed from {} threads", thread_count));
}

// exception handler
unsafe extern "system" fn exception_handler(exception_info: *mut EXCEPTION_POINTERS) -> i32 {
    if exception_info.is_null() {
        return EXCEPTION_CONTINUE_SEARCH;
    }

    let info = &*exception_info;
    let record = &*info.ExceptionRecord;

    if record.ExceptionCode == EXCEPTION_SINGLE_STEP {
        let context = &mut *info.ContextRecord;

        if let Some(target) = *TARGET_INST.lock().unwrap() {
    if context.Rip == target as u64 {
    let property_addr = context.Rax as usize;

    // capture addresses
	if CAPTURE_MODE.load(Ordering::Relaxed) {

		let mut addrs = LAST_PROP_ADDRS.lock().unwrap();

		let already_known = addrs.iter().any(|&addr| addr == property_addr);

		if !already_known {
			let now = Instant::now();

			{
				let mut last_time = LAST_CAPTURE_TIME.lock().unwrap();
	
				if let Some(last) = *last_time {
					if now.duration_since(last).as_millis() > CAPTURE_TIMEOUT_MS {
						addrs.fill(0);
						CAPTURE_SERIAL.fetch_add(1, Ordering::Relaxed);
						log_debug("New capture burst started");
					}
				}

				*last_time = Some(now);
			}

			if let Some(slot) = addrs.iter_mut().find(|addr| **addr == 0) {
				*slot = property_addr;
				CAPTURE_SERIAL.fetch_add(1, Ordering::Relaxed);
				log_debug(&format!("Property captured: 0x{:X}", property_addr));
			} else {
				log_debug(&format!("Ignored extra property: 0x{:X}", property_addr));
			}
		}
	}

    // block the write
    if BLOCK_GAME_WRITES.load(Ordering::Relaxed) {
        context.Rip += 4; // skip F3 0F 11 10 movss [rax],xmm2
        context.EFlags |= 0x10000;
        return EXCEPTION_CONTINUE_EXECUTION;
    }

    context.EFlags |= 0x10000;
    return EXCEPTION_CONTINUE_EXECUTION;
}
}

        return EXCEPTION_CONTINUE_EXECUTION;
    }

    EXCEPTION_CONTINUE_SEARCH
}

// ImGui overlay struct
struct WheelEditor {
    show: bool,
    capture_mode: bool,
    values: [f32; MAX_PROPERTIES],
	active_property_count: usize,
    last_loaded_capture: u32,
	read_on_select: bool,
	selected_property: usize,
	instant_update: bool,
}

impl WheelEditor {
	fn apply_to_selected(&self) {
    let addrs = LAST_PROP_ADDRS.lock().unwrap();

    unsafe {
        let mut written = 0usize;

        for i in 0..self.active_property_count {
            if addrs[i] == 0 {
                continue;
            }

            WriteProcessMemory(
                GetCurrentProcess(),
                addrs[i] as *mut c_void,
                &self.values[i] as *const f32 as *const c_void,
                4,
                &mut written,
            );
        }
    }
}
	fn uninit(&mut self) {
			unsafe {
				if BREAKPOINT_SET.load(Ordering::Relaxed) {
					remove_breakpoint_from_all_threads();
					BREAKPOINT_SET.store(false, Ordering::Relaxed);
				}
			}

			CAPTURE_MODE.store(false, Ordering::Relaxed);
			self.capture_mode = false;

			log_debug("Eject requested");
			eject();
		}
    fn new() -> Self {
        log_debug("=== SWRIPE Initializing ===");
        
        if let Some(base) = get_module_base() {
            let target = base + OFFSET;
            *TARGET_INST.lock().unwrap() = Some(target);
            log_debug(&format!("Target instruction address: 0x{:X}", target));

            unsafe {
                let handler = AddVectoredExceptionHandler(1, Some(exception_handler));
                log_debug(&format!("Vectored exception handler added: {:?}", handler));
            }
        } else {
            log_debug("ERROR: Failed to get module base!");
        }

        Self {
			show: true,
			capture_mode: false,
			values: [1.0; MAX_PROPERTIES],
			active_property_count: 5,
			last_loaded_capture: 0,
			read_on_select: true,
			selected_property: 0,
			instant_update: false,
		}
    }
}

impl ImguiRenderLoop for WheelEditor {
    fn render(&mut self, ui: &mut Ui) {
        unsafe {
            if GetAsyncKeyState(VK_INSERT) & 1 != 0 {
                self.show = !self.show;
                log_debug(&format!("Overlay visibility: {}", self.show));
            }

            if GetAsyncKeyState(VK_F2) & 1 != 0 {
				Beep(800, 50);
                self.capture_mode = !self.capture_mode;
                CAPTURE_MODE.store(self.capture_mode, Ordering::Relaxed);
                log_debug(&format!("Capture mode toggled: {}", self.capture_mode));

                if self.capture_mode && !BREAKPOINT_SET.load(Ordering::Relaxed) {
                    set_breakpoint_on_all_threads();
                    BREAKPOINT_SET.store(true, Ordering::Relaxed);
                } else if !self.capture_mode && BREAKPOINT_SET.load(Ordering::Relaxed) {
                    remove_breakpoint_from_all_threads();
                    BREAKPOINT_SET.store(false, Ordering::Relaxed);
                }
            }
			let mut changed_by_key = false;

if GetAsyncKeyState(VK_UP) & 1 != 0 {
    if self.selected_property > 0 {
        self.selected_property -= 1;
    }
}

if GetAsyncKeyState(VK_DOWN) & 1 != 0 {
    if self.selected_property + 1 < self.active_property_count {
        self.selected_property += 1;
    }
}

if GetAsyncKeyState(VK_RIGHT) & 1 != 0 {
    self.values[self.selected_property] += 0.1;
    changed_by_key = true;
}

if GetAsyncKeyState(VK_LEFT) & 1 != 0 {
    self.values[self.selected_property] -= 0.1;
    changed_by_key = true;
}

if changed_by_key && self.instant_update {
    self.apply_to_selected();
}
        }

        if !self.show {
            return;
        }

        ui.window("SWRIPE")
    .size([430.0, 320.0], Condition::FirstUseEver)
    .build(|| {
        ui.text("Stormworks Realtime Illegal Property Editor");
        ui.separator();
        
        let mut capture = self.capture_mode;
        if ui.checkbox("Capture Mode (F2)", &mut capture) {
            self.capture_mode = capture;
            CAPTURE_MODE.store(self.capture_mode, Ordering::Relaxed);
            unsafe {
                if self.capture_mode && !BREAKPOINT_SET.load(Ordering::Relaxed) {
                    set_breakpoint_on_all_threads();
                    BREAKPOINT_SET.store(true, Ordering::Relaxed);
                } else if !self.capture_mode && BREAKPOINT_SET.load(Ordering::Relaxed) {
                    remove_breakpoint_from_all_threads();
                    BREAKPOINT_SET.store(false, Ordering::Relaxed);
                }
            }
        }
        
        ui.separator();
		
		let mut block_writes = BLOCK_GAME_WRITES.load(Ordering::Relaxed);

		if ui.checkbox("Prevent Stormworks property writes", &mut block_writes) {
			BLOCK_GAME_WRITES.store(block_writes, Ordering::Relaxed);
		}       

		let addrs = *LAST_PROP_ADDRS.lock().unwrap();
let has_selection = addrs[0] != 0;

let capture_serial = CAPTURE_SERIAL.load(Ordering::Relaxed);

if self.read_on_select && has_selection && capture_serial != self.last_loaded_capture {
    unsafe {
        let mut read = 0usize;

        for i in 0..self.active_property_count {
            if addrs[i] == 0 {
                continue;
            }

            ReadProcessMemory(
                GetCurrentProcess(),
                addrs[i] as *const c_void,
                &mut self.values[i] as *mut f32 as *mut c_void,
                4,
                &mut read,
            );
        }

        self.last_loaded_capture = capture_serial;
    }
}

if has_selection {
    if CollapsingHeader::new("Debug").build(ui) {
        for i in 0..self.active_property_count {
            ui.text(format!("Property {} Address: 0x{:X}", i + 1, addrs[i]));
            ui.same_line();

            let button_id = format!("Copy##addr_{}", i);
            if ui.small_button(&button_id) {
                let text = format!("{:X}", addrs[i]);
                unsafe {
                    copy_to_clipboard(&text);
                }
                log_debug(&format!("Copied Property {} Address: {}", i + 1, text));
            }
        }
    }
}

else {
    ui.text("No component selected");
    ui.text("Click a component in capture mode");
}
        
ui.separator();

ui.checkbox("Read values when selecting", &mut self.read_on_select);
ui.checkbox("Instant update", &mut self.instant_update);

ui.separator();

let names: &[&str] = &[
    "Stiffness / Rotor Size / Rocket Burn Rate / Grip",
    "Damping / Rocket Fuel Amount / Radius",
    "Grip / Pressure",
    "Radius",
    "Pressure",
];

let old_count = self.active_property_count;

self.active_property_count = names.len();

if self.selected_property >= self.active_property_count {
    self.selected_property = self.active_property_count.saturating_sub(1);
}

if old_count != self.active_property_count {
    log_debug(&format!(
        "Property count changed {} -> {}",
        old_count,
        self.active_property_count
    ));
}

for i in 0..self.active_property_count {
    let label = if i == self.selected_property {
        format!("<{}>:", names[i])
    } else {
        format!("{}:", names[i])
    };

    ui.text(label);

    let input_id = format!("##prop_input_{}", i);
    let mut val = self.values[i];

    if ui.input_float(&input_id, &mut val).step(0.1).build() {
        self.values[i] = val;

        if self.instant_update {
            self.apply_to_selected();
        }
    }
}

if ui.button("Apply to Selected Component") {
    self.apply_to_selected();

    unsafe {
        Beep(1500, 50);
    }
}

ui.same_line();
        
        if ui.button("Clear Selection") {
			LAST_PROP_ADDRS.lock().unwrap().fill(0);
			*LAST_CAPTURE_TIME.lock().unwrap() = None;
			self.last_loaded_capture = 0;
		}

		ui.same_line();

		if ui.button("Eject DLL") {
			self.uninit();
		}
        
        ui.separator();
        ui.text("F2 - Toggle Capture Mode");
        ui.text("INSERT - Hide/Show Overlay");
        ui.text("Select a component with the Stormworks select tool");
        ui.text("Adjust number, then click Apply");
    });

let status = if self.capture_mode { "CAPTURING" } else { "IDLE" };
let version_text = format!("SWRIPE | {}", status);

ui.window("##status")
    .flags(WindowFlags::NO_DECORATION | WindowFlags::ALWAYS_AUTO_RESIZE)
    .position(ui.io().display_size, Condition::Always)
    .position_pivot([1.0, 1.0])
    .build(|| {
        ui.text(&version_text);
    });
    }
}

// dll entry point
#[no_mangle]
pub unsafe extern "system" fn DllMain(hmodule: HINSTANCE, reason: u32, _: *mut ()) -> BOOL {
    if reason == 1 {
        let game_module = GetModuleHandleA("Stormworks64.exe\0".as_ptr() as *const i8);

if !game_module.is_null() {
    if let Some(path) = module_dir_log_path(game_module as *mut c_void) {
        let _ = std::fs::write(&path, "");
        *GAME_LOG_PATH.lock().unwrap() = Some(path);
    }
}

if let Some(path) = module_dir_log_path(hmodule.0 as *mut c_void) {
    let _ = std::fs::write(&path, "");
    *DLL_LOG_PATH.lock().unwrap() = Some(path);
}

log_debug("=== DLL Attached ===");
        
        let hmodule_raw = hmodule.0 as usize;

        std::thread::spawn(move || {
            let hmodule = HINSTANCE(hmodule_raw as *mut std::ffi::c_void);

            if let Err(e) = Hudhook::builder()
                .with::<ImguiOpenGl3Hooks>(WheelEditor::new())
                .with_hmodule(hmodule)
                .build()
                .apply()
            {
                log_debug(&format!("Failed to apply hooks: {:?}", e));
            }
        });
    }

    TRUE
}