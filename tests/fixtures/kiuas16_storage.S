.text
.globl _start
_start:
    li sp, 1
    slli sp, sp, 31
    li t0, 65536
    add sp, sp, t0
    lui s0, 0x10000
    li a0, 70
    call add_five
    sb a0, 0(s0)
    lui s1, 0x10001
    li s2, 1
    slli s2, s2, 31
    li t0, 3
    sw t0, 0x70(s1)
    li t0, 1
    sw t0, 0x24(s1)
    sw t0, 0x20(s1)
    sw zero, 0x24(s1)
    li t0, 512
    sw t0, 0x20(s1)
    li t0, 11
    sw t0, 0x70(s1)
    li t0, 8
    sw t0, 0x38(s1)
    sw s2, 0x80(s1)
    addi t0, s2, 0x100
    sw t0, 0x90(s1)
    addi t0, s2, 0x200
    sw t0, 0xa0(s1)
    li t0, 1
    sw t0, 0x44(s1)
    li t0, 15
    sw t0, 0x70(s1)
    la t0, descriptors
    mv t1, s2
    li t2, 6
copy:
    ld t3, 0(t0)
    sd t3, 0(t1)
    addi t0, t0, 8
    addi t1, t1, 8
    addi t2, t2, -1
    bne t2, zero, copy
    li t0, 1
    sd t0, 0x308(s2)
    li t1, 255
    sb t1, 0x600(s2)
    sh t0, 0x102(s2)
    sw zero, 0x50(s1)
    lbu a0, 0x400(s2)
    sb a0, 0(s0)
    lbu a0, 0x600(s2)
    sb a0, 0(s0)
    li t0, 1
    sw t0, 0x300(s2)
    li t0, 87
    sb t0, 0x400(s2)
    li t0, 0x20001
    sw t0, 28(s2)
    li t0, 255
    sb t0, 0x600(s2)
    li t0, 2
    sh t0, 0x102(s2)
    sw zero, 0x50(s1)
    lbu a0, 0x600(s2)
    sb a0, 0(s0)
    li t0, 4
    sw t0, 0x300(s2)
    li t0, 0x20001
    sw t0, 12(s2)
    li t0, 255
    sb t0, 0x600(s2)
    li t0, 3
    sh t0, 0x102(s2)
    sw zero, 0x50(s1)
    lbu a0, 0x600(s2)
    sb a0, 0(s0)
    lw t0, 0x60(s1)
    sb t0, 0(s0)
    lhu a0, 0x202(s2)
    sb a0, 0(s0)
    sw t0, 0x64(s1)
    sw zero, 0x70(s1)
    lw a0, 0x44(s1)
    sb a0, 0(s0)
    wfi
    j _start
add_five:
    addi sp, sp, -16
    sd ra, 8(sp)
    call add_two
    addi a0, a0, 3
    ld ra, 8(sp)
    addi sp, sp, 16
    ret
add_two:
    addi a0, a0, 2
    ret
.balign 8
descriptors:
    .dword 0x80000300
    .word 16, 0x10001
    .dword 0x80000400
    .word 512, 0x20003
    .dword 0x80000600
    .word 1, 2
