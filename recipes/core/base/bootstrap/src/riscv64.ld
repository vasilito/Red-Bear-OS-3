ENTRY(_start)
OUTPUT_FORMAT(elf64-littleriscv)

SECTIONS {
  . = 4096 + 4096; /* Reserved for the null page and the initfs header prepended by redox-initfs-ar */
  __initfs_header = . - 4096;
  . += SIZEOF_HEADERS;
  . = ALIGN(4096);

  .text : {
    __text_start = .;
    *(.text*)
    . = ALIGN(4096);
    __text_end = .;
  }
  .rodata : {
    __rodata_start = .;
    *(.rodata*)
  }
  .data.rel.ro : {
    *(.data.rel.ro*)
  }
  .got : {
    *(.got)
  }
  .got.plt : {
    *(.got.plt)
    . = ALIGN(4096);
    __rodata_end = .;
  }
  .data : {
    __data_start = .;
    *(.data*)
    *(.sdata*)
    . = ALIGN(4096);
    __data_end = .;

    __bss_start = .;
    *(.bss*)
    *(.sbss*)
    . = ALIGN(4096);
    __bss_end = .;
  }

  /DISCARD/ : {
      *(.comment*)
      *(.eh_frame*)
      *(.gcc_except_table*)
      *(.note*)
      *(.rel.eh_frame*)
  }
}
