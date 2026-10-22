/* The boot loader copies the image to physical 1 MiB; the kernel runs at
   KERNEL_BASE + 1 MiB in the top 2 GiB of the address space. */
ENTRY(_start)

KERNEL_BASE = 0xFFFFFFFF80000000;

SECTIONS
{
    . = KERNEL_BASE + 0x100000;
    __kernel_start = .;

    .text : {
        KEEP(*(.text.boot))
        *(.text .text.*)
    }

    . = ALIGN(4096);
    .rodata : {
        *(.rodata .rodata.*)
    }

    . = ALIGN(4096);
    .data : {
        *(.data .data.*)
        *(.got .got.*)
    }

    .bss (NOLOAD) : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    . = ALIGN(4096);
    __kernel_end = .;

    /DISCARD/ : {
        *(.eh_frame .eh_frame_hdr .comment .note .note.*)
    }
}
