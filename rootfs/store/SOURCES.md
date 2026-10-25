# Programmi del negozio (sys:/store)

Programmi liberi compilati dai sorgenti ufficiali, collegati staticamente con
[musl](https://musl.libc.org/) per x86-64 Linux. arcOS li esegue con il suo
livello di compatibilità Linux (`kernel/src/linux.rs`). Nessuna modifica ai
sorgenti.

| Programma | Sorgente | Versione | Licenza | Comando |
|---|---|---|---|---|
| `busybox` | https://github.com/mirror/busybox | commit 371fe9f | GPL-2.0 | `musl-gcc`, `allnoconfig` + applet scelti, `CONFIG_STATIC=y` |
| `lua`     | https://github.com/lua/lua         | 5.4 (8cb93ba) | MIT | `make CC=musl-gcc MYLDFLAGS=-static` |

Per ricompilarli servono `musl-gcc` (pacchetto `musl-tools`) e `make`.
