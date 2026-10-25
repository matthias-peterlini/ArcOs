arcOS: un sistema operativo per PC x86-64 scritto da zero, con il linguaggio ArcScript.

## Novità

- **Programmi Linux**: gli eseguibili ELF x86-64 statici (musl) girano senza modifiche.
- **Programmi Windows**: i `.exe` x64 da console (MinGW-w64) girano con `winrun`, che fornisce un `kernel32`/`msvcrt` scritti da zero.
- **`install`**: installa programmi liberi dal negozio `sys:/store` — `install busybox` (decine di comandi Unix) e `install lua`.

Dettagli in [docs/compat.md](https://github.com/matthias-peterlini/ArcOs/blob/main/docs/compat.md).

## File

| File | Cosa farne |
|---|---|
| `arcos.iso` | **CD live**: si avvia in una macchina virtuale (VirtualBox, VMware, QEMU, Hyper-V di prima generazione) o da un CD, con BIOS o UEFI, senza disco rigido. Il sistema gira in memoria: le modifiche si perdono allo spegnimento. |
| `arcos-img.zip` | **Immagine di disco** (128 MiB, BIOS e UEFI): scompattata, si scrive su una chiavetta USB (balenaEtcher, Rufus in modalità DD, `dd`) per avviare un PC vero, con le modifiche che restano. Secure Boot va disattivato. |
| `SHA256SUMS` | le impronte dei due file |

## Provarlo

- **QEMU**: `qemu-system-x86_64 -m 512M -cdrom arcos.iso`
- **VirtualBox**: nuova macchina "Other/Unknown (64-bit)", 512 MiB di memoria, nessun disco, `arcos.iso` nel lettore CD.

Al prompt `help` elenca i comandi; `cd sys:/home/arcscript` e poi `hello.as` avvia un programma ArcScript; `install busybox` e poi `seq 1 5` prova un programma Linux. Documentazione nel [README](https://github.com/matthias-peterlini/ArcOs#readme).
