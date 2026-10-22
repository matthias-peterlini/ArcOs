; ArcBoot stage 2.
;
; Loaded by stage 1 at 0000:7E00, entered in real mode with the boot drive in
; DL. It:
;   1. enables the A20 line and switches to "unreal" mode (4 GiB data limit);
;   2. reads the kernel image to 1 MiB, the boot pack right after it and,
;      when booting a live CD, the system volume after that;
;   3. collects the BIOS memory map (E820) and the 8x16 VGA font;
;   4. picks a 32-bit linear framebuffer mode with VBE (text mode otherwise);
;   5. builds the initial page tables and enters 64-bit long mode;
;   6. jumps to the kernel entry with RDI = physical address of the BootInfo.
;
; Physical memory used below 1 MiB (all of it is reclaimable by the kernel):
;   0x01000  BootInfo            0x02000  E820 map       0x03000  font
;   0x07C00  stage 1             0x07E00  stage 2 (<= 32 KiB)
;   0x10000  page tables (7 pages)
;   0x20000  disk bounce buffer (32 KiB)
;   0x30000  VBE info block, 0x30200 VBE mode info

bits 16
org 0x7E00

BOOTINFO        equ 0x1000
MMAP            equ 0x2000
MMAP_MAX        equ 170
FONT            equ 0x3000
PML4            equ 0x10000
PDPT_LOW        equ 0x11000
PD0             equ 0x12000             ; four PDs: 0x12000..0x15FFF
PDPT_HIGH       equ 0x16000
BOUNCE_SEG      equ 0x2000              ; 0x20000
BOUNCE          equ 0x20000
BOUNCE_SECTORS  equ 64
VBE_SEG         equ 0x3000
KERNEL_PHYS     equ 0x100000

; Boot table inside stage 1 (see stage1.asm).
BT_SYSTEM_LBA       equ 0x7C00 + 0x184
BT_SYSTEM_SECTORS   equ 0x7C00 + 0x188
BT_STAGE2_SECTORS   equ 0x7C00 + 0x194
BT_FLAGS            equ 0x7C00 + 0x196
BOOT_CD             equ 1
BT_KERNEL_LBA       equ 0x7C00 + 0x198
BT_KERNEL_SECTORS   equ 0x7C00 + 0x19C
BT_KERNEL_MEMSZ     equ 0x7C00 + 0x1A0
BT_PACK_LBA         equ 0x7C00 + 0x1A4
BT_PACK_SECTORS     equ 0x7C00 + 0x1A8
BT_KERNEL_ENTRY     equ 0x7C00 + 0x1AC

; BootInfo field offsets (shared with the kernel's boot.rs).
BI_MAGIC        equ 0x00
BI_VERSION      equ 0x08
BI_BOOT_DRIVE   equ 0x0C
BI_MMAP_ADDR    equ 0x10
BI_MMAP_COUNT   equ 0x18
BI_FB_ADDR      equ 0x20
BI_FB_WIDTH     equ 0x28
BI_FB_HEIGHT    equ 0x2C
BI_FB_PITCH     equ 0x30
BI_FB_BPP       equ 0x34
BI_FB_KIND      equ 0x38
BI_FB_RED_POS   equ 0x3C
BI_FONT_ADDR    equ 0x48
BI_FONT_HEIGHT  equ 0x50
BI_FONT_WIDTH   equ 0x54
BI_KERNEL_PHYS  equ 0x58
BI_KERNEL_SIZE  equ 0x60
BI_PACK_PHYS    equ 0x68
BI_PACK_SIZE    equ 0x70
BI_RSDP         equ 0x78
BI_LOADER       equ 0x80
BI_SYSTEM_PHYS  equ 0x88
BI_SYSTEM_SIZE  equ 0x90

FB_NONE         equ 0
FB_RGB          equ 1
FB_TEXT         equ 2

stage2:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    sti
    cld
    mov [boot_drive], dl

    mov si, msg_stage2
    call print

    ; Clear the BootInfo page.
    mov di, BOOTINFO
    mov cx, 2048
    xor ax, ax
    rep stosw
    mov dword [BOOTINFO + BI_MAGIC], 0x42435241        ; "ARCBOOT1"
    mov dword [BOOTINFO + BI_MAGIC + 4], 0x31544F4F
    mov dword [BOOTINFO + BI_VERSION], 1
    movzx eax, byte [boot_drive]
    mov [BOOTINFO + BI_BOOT_DRIVE], eax
    mov dword [BOOTINFO + BI_LOADER], 1                 ; BIOS

    call enable_a20
    call check_long_mode
    call enter_unreal

    ; --- Kernel ----------------------------------------------------------
    mov si, msg_kernel
    call print
    mov eax, [BT_KERNEL_LBA]
    mov [load_lba], eax
    mov eax, [BT_KERNEL_SECTORS]
    test eax, eax
    jz err_nokernel
    mov [load_left], eax
    mov dword [load_dest], KERNEL_PHYS
    call load_image
    mov dword [BOOTINFO + BI_KERNEL_PHYS], KERNEL_PHYS
    mov eax, [BT_KERNEL_MEMSZ]
    mov [BOOTINFO + BI_KERNEL_SIZE], eax

    ; --- Boot pack: right after the kernel (including .bss), page aligned.
    mov eax, KERNEL_PHYS
    add eax, [BT_KERNEL_MEMSZ]
    add eax, 0xFFF
    and eax, 0xFFFFF000
    mov [BOOTINFO + BI_PACK_PHYS], eax
    mov [load_dest], eax
    mov eax, [BT_PACK_SECTORS]
    test eax, eax
    jz .no_pack
    mov si, msg_pack
    call print
    mov [load_left], eax
    shl eax, 9
    mov [BOOTINFO + BI_PACK_SIZE], eax
    mov eax, [BT_PACK_LBA]
    mov [load_lba], eax
    call load_image
.no_pack:

    ; --- Live system volume (a CD): after the boot pack, page aligned.
    mov eax, [BT_SYSTEM_SECTORS]
    test eax, eax
    jz .no_system
    mov si, msg_system
    call print
    mov [load_left], eax
    shl eax, 9
    mov [BOOTINFO + BI_SYSTEM_SIZE], eax
    mov eax, [BOOTINFO + BI_PACK_PHYS]
    add eax, [BOOTINFO + BI_PACK_SIZE]
    add eax, 0xFFF
    and eax, 0xFFFFF000
    mov [BOOTINFO + BI_SYSTEM_PHYS], eax
    mov [load_dest], eax
    mov eax, [BT_SYSTEM_LBA]
    mov [load_lba], eax
    call load_image
.no_system:

    call read_memory_map
    call read_font
    call find_rsdp

    mov si, msg_ok
    call print

    call setup_video                ; last: the screen may stop showing text

    ; --- Into long mode ----------------------------------------------------
    cli
    call build_page_tables
    lgdt [gdt_desc]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp CODE32:pmode

; ---------------------------------------------------------------------------
; Real-mode helpers

; print: SI = zero-terminated string (BIOS teletype)
print:
    pusha
.next:
    lodsb
    test al, al
    jz .done
    mov ah, 0x0E
    mov bx, 0x0007
    int 0x10
    jmp .next
.done:
    popa
    ret

fatal:                              ; SI = message
    call print
.hang:
    cli
    hlt
    jmp .hang

err_disk:
    mov si, msg_err_disk
    jmp fatal
err_nokernel:
    mov si, msg_err_nokernel
    jmp fatal
err_cpu:
    mov si, msg_err_cpu
    jmp fatal

enable_a20:
    mov ax, 0x2401                  ; BIOS method
    int 0x15
    in al, 0x92                     ; "fast A20" as a fallback
    test al, 2
    jnz .done
    or al, 2
    and al, 0xFE
    out 0x92, al
.done:
    ret

; Long mode needs CPUID leaf 0x80000001 with the LM bit.
check_long_mode:
    pushfd
    pop eax
    mov ecx, eax
    xor eax, 1 << 21                ; can we flip the ID flag?
    push eax
    popfd
    pushfd
    pop eax
    push ecx
    popfd
    xor eax, ecx
    jz err_cpu
    mov eax, 0x80000000
    cpuid
    cmp eax, 0x80000001
    jb err_cpu
    mov eax, 0x80000001
    cpuid
    test edx, 1 << 29
    jz err_cpu
    ret

; Load DS and ES with 4 GiB-limit descriptors, then return to real mode: the
; cached limits stay, so 32-bit addresses work with a 0x67 prefix. BIOS calls
; may reload the segments, so this runs again before every copy.
enter_unreal:
    cli
    push ds
    push es
    lgdt [gdt_desc]
    mov eax, cr0
    or al, 1
    mov cr0, eax
    jmp $ + 2
    mov bx, DATA32
    mov ds, bx
    mov es, bx
    and al, 0xFE
    mov cr0, eax
    pop es
    pop ds
    sti
    ret

; load_image: reads load_left sectors from load_lba to physical load_dest
; (512-byte units; on a CD they become 2048-byte sectors, rounded up).
load_image:
    mov word [chunk], BOUNCE_SECTORS
    mov byte [dword_shift], 7
    test byte [BT_FLAGS], BOOT_CD
    jz .loop
    shr dword [load_lba], 2
    add dword [load_left], 3
    shr dword [load_left], 2
    mov word [chunk], BOUNCE_SECTORS / 4
    mov byte [dword_shift], 9
.loop:
    mov eax, [load_left]
    test eax, eax
    jz .done
    movzx ecx, word [chunk]
    cmp eax, ecx
    jbe .count_ok
    mov eax, ecx
.count_ok:
    mov [dap_count], ax
    mov word [dap_offset], 0
    mov word [dap_segment], BOUNCE_SEG
    mov ebx, [load_lba]
    mov [dap_lba], ebx
    mov dword [dap_lba + 4], 0
    mov si, dap
    mov ah, 0x42
    mov dl, [boot_drive]
    int 0x13
    jc err_disk

    call enter_unreal
    movzx eax, word [dap_count]
    mov cl, [dword_shift]
    shl eax, cl                     ; sectors -> dwords
    mov ecx, eax
    mov esi, BOUNCE
    mov edi, [load_dest]
    a32 rep movsd
    mov [load_dest], edi

    movzx eax, word [dap_count]
    add [load_lba], eax
    sub [load_left], eax
    mov al, '.'
    mov ah, 0x0E
    mov bx, 7
    int 0x10
    jmp .loop
.done:
    mov si, msg_crlf
    call print
    ret

read_memory_map:
    xor ebx, ebx
    xor bp, bp                      ; entry count
    mov di, MMAP
.next:
    mov dword [di + 20], 1          ; ACPI 3.0 "valid" bit, in case the BIOS skips it
    mov eax, 0xE820
    mov ecx, 24
    mov edx, 0x534D4150             ; "SMAP"
    int 0x15
    jc .done
    cmp eax, 0x534D4150
    jne .done
    jcxz .skip
    cmp cl, 20
    jbe .keep
    test byte [di + 20], 1          ; extended entry marked "ignore"
    jz .skip
.keep:
    mov ecx, [di + 8]
    or ecx, [di + 12]
    jz .skip                        ; zero-length entry
    inc bp
    add di, 24
    cmp bp, MMAP_MAX
    jae .done
.skip:
    test ebx, ebx
    jnz .next
.done:
    mov dword [BOOTINFO + BI_MMAP_ADDR], MMAP
    movzx eax, bp
    mov [BOOTINFO + BI_MMAP_COUNT], eax
    test bp, bp
    jnz .ok
    mov si, msg_err_mmap
    jmp fatal
.ok:
    ret

; Copy the BIOS 8x16 font so the kernel can draw text on a framebuffer.
read_font:
    push ds
    push es
    mov ax, 0x1130
    mov bh, 6
    int 0x10                        ; ES:BP -> font
    push es
    pop ds
    mov si, bp
    xor ax, ax
    mov es, ax
    mov di, FONT
    mov cx, 2048
    rep movsw
    pop es
    pop ds
    mov dword [BOOTINFO + BI_FONT_ADDR], FONT
    mov dword [BOOTINFO + BI_FONT_HEIGHT], 16
    mov dword [BOOTINFO + BI_FONT_WIDTH], 8
    ret

; ACPI RSDP: "RSD PTR " on a 16-byte boundary in the first KiB of the EBDA or
; in 0xE0000..0xFFFFF.
find_rsdp:
    push es
    mov ax, [0x40E]                 ; EBDA segment
    mov es, ax
    xor di, di
    mov cx, 64
.ebda:
    call .check
    jc .found
    add di, 16
    loop .ebda
    mov ax, 0xE000
    mov es, ax
    xor di, di
.bios:
    call .check
    jc .found
    add di, 16
    jnz .bios
    mov ax, es
    add ax, 0x1000
    mov es, ax
    cmp ax, 0
    jne .bios
    pop es
    ret
.found:
    mov ax, es
    movzx eax, ax
    shl eax, 4
    movzx edi, di
    add eax, edi
    mov [BOOTINFO + BI_RSDP], eax
    pop es
    ret
.check:                             ; CF = 1 if ES:DI holds the signature
    cmp dword [es:di], 0x20445352   ; "RSD "
    jne .no
    cmp dword [es:di + 4], 0x20525450   ; "PTR "
    jne .no
    stc
    ret
.no:
    clc
    ret

; Pick a VBE mode: 1024x768x32 if offered, otherwise the largest 32-bit mode
; up to 1280x1024. Without one the kernel keeps the 80x25 text screen.
setup_video:
    mov dword [BOOTINFO + BI_FB_KIND], FB_TEXT
    mov dword [BOOTINFO + BI_FB_ADDR], 0xB8000
    mov dword [BOOTINFO + BI_FB_WIDTH], 80
    mov dword [BOOTINFO + BI_FB_HEIGHT], 25
    mov dword [BOOTINFO + BI_FB_PITCH], 160
    mov dword [BOOTINFO + BI_FB_BPP], 16

    push es
    mov ax, VBE_SEG
    mov es, ax
    xor di, di
    mov dword [es:0], 0x32454256    ; "VBE2"
    mov ax, 0x4F00
    int 0x10
    cmp ax, 0x004F
    jne .done
    mov si, [es:0x0E]               ; mode list (offset:segment)
    mov ax, [es:0x10]
    mov [mode_list_seg], ax
    mov word [best_mode], 0xFFFF
    mov dword [best_area], 0
.next_mode:
    push es
    mov ax, [mode_list_seg]
    mov es, ax
    mov cx, [es:si]
    pop es
    cmp cx, 0xFFFF
    je .chosen
    add si, 2
    mov [cur_mode], cx
    mov ax, 0x4F01
    mov di, 0x200
    int 0x10
    cmp ax, 0x004F
    jne .next_mode
    mov ax, [es:0x200]              ; attributes: supported + graphics + LFB
    and ax, 0x0091
    cmp ax, 0x0091
    jne .next_mode
    cmp byte [es:0x200 + 0x19], 32  ; bits per pixel
    jne .next_mode
    cmp byte [es:0x200 + 0x1B], 6   ; direct colour
    jne .next_mode
    mov ax, [es:0x200 + 0x12]       ; width
    mov bx, [es:0x200 + 0x14]       ; height
    cmp ax, 1024
    jne .not_preferred
    cmp bx, 768
    jne .not_preferred
    mov cx, [cur_mode]
    mov [best_mode], cx
    jmp .chosen
.not_preferred:
    cmp ax, 1280
    ja .next_mode
    cmp bx, 1024
    ja .next_mode
    movzx eax, ax
    movzx ebx, bx
    imul eax, ebx
    cmp eax, [best_area]
    jbe .next_mode
    mov [best_area], eax
    mov cx, [cur_mode]
    mov [best_mode], cx
    jmp .next_mode
.chosen:
    mov cx, [best_mode]
    cmp cx, 0xFFFF
    je .done
    mov ax, 0x4F01                  ; reload the chosen mode's info
    mov di, 0x200
    int 0x10
    cmp ax, 0x004F
    jne .done
    mov ax, 0x4F02
    mov bx, [best_mode]
    or bx, 0x4000                   ; linear framebuffer
    int 0x10
    cmp ax, 0x004F
    jne .done
    mov dword [BOOTINFO + BI_FB_KIND], FB_RGB
    mov eax, [es:0x200 + 0x28]
    mov [BOOTINFO + BI_FB_ADDR], eax
    movzx eax, word [es:0x200 + 0x12]
    mov [BOOTINFO + BI_FB_WIDTH], eax
    movzx eax, word [es:0x200 + 0x14]
    mov [BOOTINFO + BI_FB_HEIGHT], eax
    movzx eax, word [es:0x200 + 0x10]
    mov [BOOTINFO + BI_FB_PITCH], eax
    mov dword [BOOTINFO + BI_FB_BPP], 32
    mov al, [es:0x200 + 0x20]       ; red position
    mov [BOOTINFO + BI_FB_RED_POS], al
    mov al, [es:0x200 + 0x1F]       ; red size
    mov [BOOTINFO + BI_FB_RED_POS + 1], al
    mov al, [es:0x200 + 0x22]
    mov [BOOTINFO + BI_FB_RED_POS + 2], al
    mov al, [es:0x200 + 0x21]
    mov [BOOTINFO + BI_FB_RED_POS + 3], al
    mov al, [es:0x200 + 0x24]
    mov [BOOTINFO + BI_FB_RED_POS + 4], al
    mov al, [es:0x200 + 0x23]
    mov [BOOTINFO + BI_FB_RED_POS + 5], al
.done:
    pop es
    ret

; Page tables for the jump into the kernel (2 MiB pages):
;   0x0000000000000000 + 4 GiB -> physical 0      (identity)
;   0xFFFF800000000000 + 4 GiB -> physical 0      (direct map)
;   0xFFFFFFFF80000000 + 2 GiB -> physical 0      (kernel image)
; The kernel replaces them with its own tables early on.
build_page_tables:
    call enter_unreal
    cli
    mov edi, PML4
    mov ecx, 7 * 1024
    xor eax, eax
    a32 rep stosd

    mov dword [dword PML4 + 0 * 8], PDPT_LOW | 3
    mov dword [dword PML4 + 256 * 8], PDPT_LOW | 3
    mov dword [dword PML4 + 511 * 8], PDPT_HIGH | 3
    mov dword [dword PDPT_LOW + 0 * 8], (PD0 + 0x0000) | 3
    mov dword [dword PDPT_LOW + 1 * 8], (PD0 + 0x1000) | 3
    mov dword [dword PDPT_LOW + 2 * 8], (PD0 + 0x2000) | 3
    mov dword [dword PDPT_LOW + 3 * 8], (PD0 + 0x3000) | 3
    mov dword [dword PDPT_HIGH + 510 * 8], (PD0 + 0x0000) | 3
    mov dword [dword PDPT_HIGH + 511 * 8], (PD0 + 0x1000) | 3

    ; 2048 entries across the four PDs, physical address = index * 2 MiB.
    mov edi, PD0
    xor ecx, ecx
.fill:
    mov eax, ecx
    shl eax, 21
    or eax, 0x83                    ; present, writable, 2 MiB page
    mov [dword edi], eax
    mov eax, ecx
    shr eax, 11                     ; high dword of index * 2 MiB
    mov [dword edi + 4], eax
    add edi, 8
    inc ecx
    cmp ecx, 2048
    jb .fill
    ret

; ---------------------------------------------------------------------------
bits 32
pmode:
    mov ax, DATA32
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov esp, 0x7C00

    mov eax, cr4
    or eax, (1 << 5)                ; PAE
    mov cr4, eax
    mov eax, PML4
    mov cr3, eax
    mov ecx, 0xC0000080             ; EFER
    rdmsr
    or eax, (1 << 8)                ; LME
    wrmsr
    mov eax, cr0
    or eax, (1 << 31) | 1           ; paging + protection
    mov cr0, eax
    jmp CODE64:long_mode

bits 64
long_mode:
    mov ax, DATA32
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor ax, ax
    mov fs, ax
    mov gs, ax
    mov rsp, 0x7C00
    mov edi, BOOTINFO
    mov rax, [BT_KERNEL_ENTRY]
    jmp rax

; ---------------------------------------------------------------------------
align 8
gdt:
    dq 0
    dq 0x00CF9A000000FFFF           ; 0x08: 32-bit code
    dq 0x00CF92000000FFFF           ; 0x10: data, 4 GiB
    dq 0x00AF9A000000FFFF           ; 0x18: 64-bit code
gdt_end:
gdt_desc:
    dw gdt_end - gdt - 1
    dd gdt

CODE32  equ 0x08
DATA32  equ 0x10
CODE64  equ 0x18

align 4
dap:
    db 16, 0
dap_count   dw 0
dap_offset  dw 0
dap_segment dw 0
dap_lba     dq 0

boot_drive      db 0
load_lba        dd 0
load_left       dd 0
load_dest       dd 0
chunk           dw 0                ; sectors per read
dword_shift     db 0                ; sector size in dwords, as a shift
mode_list_seg   dw 0
cur_mode        dw 0
best_mode       dw 0
best_area       dd 0

msg_stage2       db "stage 2", 13, 10, 0
msg_kernel       db "kernel ", 0
msg_pack         db "boot pack ", 0
msg_system       db "system volume ", 0
msg_ok           db "starting arcOS", 13, 10, 0
msg_crlf         db 13, 10, 0
msg_err_disk     db "disk read error", 0
msg_err_nokernel db "no kernel in the boot table", 0
msg_err_cpu      db "this CPU is not 64-bit", 0
msg_err_mmap     db "no memory map", 0
