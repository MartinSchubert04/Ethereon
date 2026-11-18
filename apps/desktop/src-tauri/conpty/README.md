# Windows Terminal's ConPTY

`conpty.dll` and `OpenConsole.exe` from Microsoft's official package
[`Microsoft.Windows.Console.ConPTY`](https://www.nuget.org/packages/Microsoft.Windows.Console.ConPTY)
1.25.260930003 (x64), MIT license (`LICENSE`). Signed by Microsoft Corporation.

Why: the ConPTY that ships with Windows reinterprets the shell's output and discards what it doesn't
understand, such as Sixel images. This is Windows Terminal's and it lets them through.
`portable-pty` uses a `conpty.dll` sitting next to the executable before Windows' own;
`tauri.conf.json` copies them there (`bundle.resources`).

To update: download the `.nupkg` from NuGet, open it as a zip and copy
`runtimes/win-x64/native/conpty.dll` and `build/native/runtimes/x64/OpenConsole.exe`.

SHA-256 (1.25.260930003):

    feeef341d891643c62d30b6b07800bc70f0bc148f44cb8c3bee84aa557ae805a  conpty.dll
    3d66b23d0a71bb8eed2b77edc8b9df9bf54ce6c8fb74c863a30e760997f80586  OpenConsole.exe
