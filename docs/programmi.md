# Programmi per arcOS

Un programma arcOS è un file `.app`: un eseguibile ELF64 che gira in
modalità utente (ring 3), nel suo spazio di indirizzi. Parla con il kernel
solo attraverso le chiamate di sistema (`int 0x80`). Il kernel lo carica a
`0x400000`, gli dà uno stack sotto `0x7FFF_FFF0_0000` e la memoria che
chiede a partire da `0x1000_0000_0000`.

Se il programma fa un accesso non valido, il kernel lo ferma e lo segnala.
Il resto del sistema continua normalmente. Ctrl+C ferma il programma in
primo piano.

## Avviarli

La shell cerca i programmi in `sys:/apps`, quindi basta il nome:

```
hello mondo
calc 6 * 7
edit sys:/home/nota.txt
run sys:/apps/info.app
ps                       programmi e thread in esecuzione
kill <pid>
```

I programmi si possono scrivere anche in ArcScript: la shell avvia i
sorgenti `.as` e i pacchetti `.arc` attraverso `arcc`, e cerca anche
questi in `sys:/apps` (`notes` è `sys:/apps/notes.as`). Vedi
[arcscript.md](arcscript.md).

## Scriverne uno

Ogni cartella in `system/apps/` è un programma. Il `Makefile` la compila e
la copia nel volume di sistema come `sys:/apps/<nome>.app`.

`Cargo.toml`:

```toml
[package]
name = "saluto"
description = "arcOS program: saluto"
version = "1.0.0"
edition.workspace = true
publish.workspace = true
authors.workspace = true

[[bin]]
name = "saluto"
path = "src/main.rs"
test = false
bench = false

[dependencies]
arcrt.workspace = true

[lints]
workspace = true
```

`build.rs`:

```rust
fn main() {
    println!("cargo:rustc-link-arg-bins=-e");
    println!("cargo:rustc-link-arg-bins=_start");
    println!("cargo:rustc-link-arg-bins=--strip-all");
    println!("cargo:rustc-link-arg-bins=-z");
    println!("cargo:rustc-link-arg-bins=max-page-size=4096");
}
```

`src/main.rs`:

```rust
#![no_std]
#![no_main]

extern crate alloc;

use arcrt::console::color::*;
use arcrt::println;

arcrt::main!(main);

/// `args` è il resto della riga di comando. Il valore restituito è il
/// codice d'uscita.
fn main(args: &str) -> i32 {
    let nome = args.trim();
    let nome = if nome.is_empty() { "mondo" } else { nome };
    println!("{ACCENT}Ciao, {nome}!{RESET}");

    // File: percorsi con il volume ("sys:/home/...") o relativi alla
    // cartella corrente.
    let testo = alloc::format!("salutato {nome}\n");
    if let Err(e) = arcrt::fs::write("sys:/home/saluti.txt", testo.as_bytes()) {
        println!("non riesco a scrivere: {e:?}");
        return 1;
    }
    0
}
```

`arcrt` contiene anche l'allocatore (si possono usare `Vec`, `String`,
`Box`) e il gestore dei panic.

### La libreria `arcrt`

| Modulo | Contenuto |
|---|---|
| `console` | `print!`, `println!`, `eprintln!`, `read_key`, `poll_key` (con timeout), `read_line`, `size`, `clear`, `goto`, colori in `console::color` |
| `fs` | `File` (`open`, `create`, `append`, `read`, `write`, `seek`, `size`, `read_to_end`), `read`, `read_to_string`, `write`, `list_dir`, `make_dir`, `remove`, `rename`, `current_dir`, `set_current_dir` |
| `process` | `exit`, `spawn`, `wait`, `yield_now`, `sysinfo` |
| `time` | `uptime_ms`, `sleep_ms`, `now`, `date` |
| radice | `main!`, `split_args` (argomenti, con le virgolette) |

### Le chiamate di sistema

Numero in `rax`, argomenti in `rdi`, `rsi`, `rdx`, `r10`, `r8`; il
risultato torna in `rax` (negativo = errore, vedi `arcdk::syscall::Error`).
File e cartelle aperti sono **handle** numerici, non descrittori Unix.

| N. | Nome | Cosa fa |
|---|---|---|
| 0 | `EXIT` | termina il programma |
| 1 | `WRITE` | scrive su un handle (la console o un file) |
| 2 | `READ_KEY` | legge un tasto (con attesa massima) |
| 3 | `OPEN` | apre o crea un file, restituisce un handle |
| 4 | `READ` | legge da un handle |
| 5 | `CLOSE` | chiude un handle |
| 6 | `SEEK` | sposta la posizione |
| 7 | `FILE_SIZE` | dimensione del file |
| 8 | `LIST_DIR` | contenuto di una cartella |
| 9 | `REMOVE` | cancella un file o una cartella vuota |
| 10 | `MAKE_DIR` | crea una cartella |
| 11 | `UPTIME` | millisecondi dall'avvio |
| 12 | `SLEEP` | attende |
| 13 | `NOW` | data e ora (secondi dal 2000) |
| 14 | `SPAWN` | avvia un altro programma |
| 15 | `WAIT` | attende la fine di un programma |
| 16 | `YIELD` | cede il processore |
| 17 | `MAP` | aggiunge memoria allo spazio del programma |
| 18 | `SYSINFO` | memoria, processore, versione |
| 19 | `SCREEN` | dimensione della console, cursore, pulizia |
| 20 | `RENAME` | rinomina o sposta |
| 21 | `GET_CWD` | cartella corrente |
| 22 | `SET_CWD` | cambia la cartella corrente |

### Copiare un programma su un'immagine esistente

```sh
cd system && cargo build --release -p saluto && cd ..
tools/target/release/arctool fs build/arcos.img put \
    system/target/x86_64-unknown-none/release/saluto /apps/saluto.app
```
