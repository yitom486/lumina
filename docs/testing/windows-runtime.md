# Windows runtime acceptance

This document separates desktop loader failures from Rust/business-test failures.
It does not suppress failed tests or mark them as passed.

## Reproducible gates

Run from the repository root in PowerShell:

```powershell
cargo fmt --all --check
cargo check -p lumina-app --lib
cargo clippy -p lumina-app --lib -- -D warnings
cargo test -p lumina-app --test windows_runtime_gate
cargo test -p lumina-app --lib
```

`windows_runtime_gate` is a standalone integration-test process. It does not
link the `lumina-app` library; it only loads the staged `libmpv-2.dll` and
reports a stable Win32 error if that native runtime or one of its dependencies
is missing. This keeps the native playback gate separate from the Tauri test
harness loader.

## `STATUS_ENTRYPOINT_NOT_FOUND` diagnosis

If `cargo test -p lumina-app --lib` shows a Windows dialog for
`lumina_lib-*.exe` before any Rust test starts, inspect the test executable's
PE imports and resources:

```powershell
$testExe = Get-ChildItem target\debug\deps\lumina_lib-*.exe | Select-Object -First 1
llvm-objdump -p $testExe.FullName | Select-String 'Resource Directory|TaskDialogIndirect|comctl32.dll'
llvm-readobj --coff-exports C:\Windows\System32\comctl32.dll | Select-String 'TaskDialogIndirect|SetWindowSubclass'
```

The observed failure on the current machine is:

1. `lumina_lib-*.exe` has no PE resource directory, so it has no Tauri
   common-controls v6 activation manifest;
2. the same test executable imports `comctl32.dll!TaskDialogIndirect` through
   Tauri's Windows dialog/runtime path;
3. the system `comctl32.dll` is version 5.82 and does not export that symbol
   without the v6 activation context;
4. the runnable `lumina-app.exe` has a resource directory and the same import,
   which explains why the packaged/dev app and `cargo test --lib` can behave
   differently.

Therefore this dialog means the test process failed during Windows loader
initialization. It is not a SQLite assertion failure and not evidence that the
chapter worker returned invalid data. Lumina keeps the cross-platform Tauri
features needed by the app but does not enable Tauri's `common-controls-v6`
feature in the test-linked dependency set; the real app still registers the
desktop dialog plugin. Deleting/ignoring tests is not an acceptable fix.

## Log directory fallback

The rolling file appender is built through its fallible builder. If the normal
`%APPDATA%\lumina\logs` directory cannot be created or opened, startup retries
under `%TEMP%\lumina\logs`; if both locations fail, stdout logging remains
active. A log-directory permission error must not panic before the Tauri window
is created.

## Manual crash/restart acceptance

1. Start the desktop app with `bun run tauri` and verify normal playback.
2. Confirm the startup marker is written as unclean before native-player
   initialization and becomes clean only on the normal shutdown path.
3. In a disposable development session, reproduce a native process exit only
   when the native runtime is already known to be unstable; do not continue
   operating after an access violation.
4. Relaunch the app and verify that the UI shows only the business message
   about the previous playback process exiting unexpectedly.
5. Verify that exception code, address, thread id, and player phase remain in
   local diagnostics only; none are rendered in the UI.
