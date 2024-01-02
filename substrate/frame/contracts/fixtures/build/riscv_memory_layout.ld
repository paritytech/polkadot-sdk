SECTIONS {
    . = 0x10000;
    .rodata : { *(.rodata) *(.rodata.*) }
    .data.rel.ro : { *(.data.rel.ro) *(.data.rel.ro.*) }
    .got : { *(.got) *(.got.*) }

    . = ALIGN(0x4000);
    .data : { *(.sdata) *(.data) }
    .bss : { *(.sbss) *(.bss) *(.bss.*) }

    . = 0xf0000000;

    .text : { KEEP(*(.text.polkavm_export)) *(.text .text.*) }

    /DISCARD/ : { *(.eh_frame) }
    . = ALIGN(4);
}

