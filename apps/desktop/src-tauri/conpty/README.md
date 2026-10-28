# ConPTY de Windows Terminal

`conpty.dll` y `OpenConsole.exe` del paquete oficial de Microsoft
[`Microsoft.Windows.Console.ConPTY`](https://www.nuget.org/packages/Microsoft.Windows.Console.ConPTY)
1.25.260930003 (x64), licencia MIT (`LICENSE`). Firmados por Microsoft Corporation.

Por qué: la ConPTY que trae Windows reinterpreta la salida de la shell y descarta lo que no
entiende, como las imagenes Sixel. Esta es la de Windows Terminal y las deja pasar.
`portable-pty` usa un `conpty.dll` que este al lado del ejecutable antes que el de Windows;
`tauri.conf.json` los copia ahi (`bundle.resources`).

Para actualizar: bajar el `.nupkg` de NuGet, abrirlo como zip y copiar
`runtimes/win-x64/native/conpty.dll` y `build/native/runtimes/x64/OpenConsole.exe`.

SHA-256 (1.25.260930003):

    feeef341d891643c62d30b6b07800bc70f0bc148f44cb8c3bee84aa557ae805a  conpty.dll
    3d66b23d0a71bb8eed2b77edc8b9df9bf54ce6c8fb74c863a30e760997f80586  OpenConsole.exe
