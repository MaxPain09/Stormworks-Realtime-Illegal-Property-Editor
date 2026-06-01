#include <windows.h>
#include <tlhelp32.h>
#include <iostream>
#include <string>

DWORD GetProcessId(const wchar_t* name) {
    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if (snapshot == INVALID_HANDLE_VALUE)
        return 0;

    PROCESSENTRY32W pe{};
    pe.dwSize = sizeof(pe);

    DWORD pid = 0;

    if (Process32FirstW(snapshot, &pe)) {
        do {
            if (_wcsicmp(pe.szExeFile, name) == 0) {
                pid = pe.th32ProcessID;
                break;
            }
        } while (Process32NextW(snapshot, &pe));
    }

    CloseHandle(snapshot);
    return pid;
}

bool IsModuleLoaded(DWORD pid, const wchar_t* moduleName) {
    HANDLE snapshot = CreateToolhelp32Snapshot(
        TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32,
        pid
    );

    if (snapshot == INVALID_HANDLE_VALUE)
        return false;

    MODULEENTRY32W me{};
    me.dwSize = sizeof(me);

    bool found = false;

    if (Module32FirstW(snapshot, &me)) {
        do {
            if (_wcsicmp(me.szModule, moduleName) == 0) {
                found = true;
                break;
            }
        } while (Module32NextW(snapshot, &me));
    }

    CloseHandle(snapshot);
    return found;
}

void PrintLastError(const char* prefix) {
    DWORD error = GetLastError();

    LPSTR buffer = nullptr;

    FormatMessageA(
        FORMAT_MESSAGE_ALLOCATE_BUFFER |
        FORMAT_MESSAGE_FROM_SYSTEM |
        FORMAT_MESSAGE_IGNORE_INSERTS,
        nullptr,
        error,
        MAKELANGID(LANG_NEUTRAL, SUBLANG_DEFAULT),
        (LPSTR)&buffer,
        0,
        nullptr
    );

    std::cout << prefix << "\n";
    std::cout << "Error " << error << ": ";

    if (buffer) {
        std::cout << buffer;
        LocalFree(buffer);
    }

    std::cout << std::endl;
}

int main() {
    DWORD pid = GetProcessId(L"Stormworks64.exe");

    if (!pid) {
        std::cout << "Stormworks64.exe not found.\n";
        system("pause");
        return 1;
    }

    std::cout << "Found Stormworks64.exe (PID: " << pid << ")\n";

    if (IsModuleLoaded(pid, L"swripe.dll")) {
        std::cout << "swripe.dll is already loaded.\n";
        system("pause");
        return 1;
    }

    HANDLE hProcess = OpenProcess(PROCESS_ALL_ACCESS, FALSE, pid);

    if (!hProcess) {
        PrintLastError("Failed to open process.");
        system("pause");
        return 1;
    }

    char dllPath[MAX_PATH];

    if (!GetCurrentDirectoryA(MAX_PATH, dllPath)) {
        PrintLastError("GetCurrentDirectoryA failed.");
        CloseHandle(hProcess);
        system("pause");
        return 1;
    }

    strcat_s(dllPath, "\\swripe.dll");

    DWORD attrs = GetFileAttributesA(dllPath);

    if (attrs == INVALID_FILE_ATTRIBUTES) {
        std::cout << "DLL not found:\n" << dllPath << std::endl;
        CloseHandle(hProcess);
        system("pause");
        return 1;
    }

    std::cout << "Injecting:\n" << dllPath << "\n";

    LPVOID remoteMem = VirtualAllocEx(
        hProcess,
        nullptr,
        MAX_PATH,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE
    );

    if (!remoteMem) {
        PrintLastError("VirtualAllocEx failed.");
        CloseHandle(hProcess);
        system("pause");
        return 1;
    }

    SIZE_T written = 0;

    if (!WriteProcessMemory(
        hProcess,
        remoteMem,
        dllPath,
        strlen(dllPath) + 1,
        &written
    )) {
        PrintLastError("WriteProcessMemory failed.");

        VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
        CloseHandle(hProcess);

        system("pause");
        return 1;
    }

    HANDLE hThread = CreateRemoteThread(
        hProcess,
        nullptr,
        0,
        (LPTHREAD_START_ROUTINE)LoadLibraryA,
        remoteMem,
        0,
        nullptr
    );

    if (!hThread) {
        PrintLastError("CreateRemoteThread failed.");

        VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
        CloseHandle(hProcess);

        system("pause");
        return 1;
    }

    WaitForSingleObject(hThread, INFINITE);

    DWORD remoteResult = 0;
    GetExitCodeThread(hThread, &remoteResult);

    if (remoteResult == 0) {
        std::cout << "LoadLibraryA failed inside Stormworks64.\n";

        CloseHandle(hThread);
        VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
        CloseHandle(hProcess);

        system("pause");
        return 1;
    }

    std::cout << "DLL injected successfully!\n";

    CloseHandle(hThread);
    VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
    CloseHandle(hProcess);

    Sleep(1000);
    return 0;
}