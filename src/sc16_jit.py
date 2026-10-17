"""Optional Numba-compiled CPU core for the SC-16 emulator.

`run_block` executes instructions exactly like `SuomiCompute16.step()`, but compiled to
machine code. It handles the instruction set (classic opcodes, extended ALU, MUL/DIV,
far branches and jumps) and stops ("bails out") *before* executing an instruction that
needs the Python emulator: a store to a device command register, HALT and floating-point
instructions. The caller then runs that one
instruction with `step()` and resumes. After an instruction that leaves an interrupt
pending the loop returns so Python can dispatch it.

State is passed in `regs` (8 registers) and `st` = [pc, sp, Z, C, N, V].
Returns (executed instruction count, status): 0 = budget used up,
1 = next instruction must be run by step(), 2 = check interrupts.
"""
import numpy as np
from numba import njit

# Must match the constants in SuomiCPU.py (checked by SuomiCPU before enabling the core).
MEM_MASK = 2**30 - 1
GPU_BASE = 0x43020
GPU_LAYER_CONTROL_START = GPU_BASE + 16
GPU_LAYER_CONTROL_END = GPU_BASE + 28
ICR_ADDR = 0x43002
DISK_BASE = 0x43100
CONSOLE_DATA = 0x43200
CONSOLE_STATUS = 0x43201
CONSOLE_CONTROL = 0x43202
MOUSE_EVENTS_ADDR = 0x4300D
FAR_CONTROL_MASK = 0xF83F
FAR_CONTROL_PREFIX = 0xF83E
FAR_BRANCH_MASK = 0xF80F
FAR_BRANCH_PREFIX = 0xF80D
CONSTANTS = (
    MEM_MASK,
    GPU_BASE,
    ICR_ADDR,
    DISK_BASE,
    CONSOLE_DATA,
    CONSOLE_STATUS,
    CONSOLE_CONTROL,
    MOUSE_EVENTS_ADDR,
    GPU_LAYER_CONTROL_START,
    GPU_LAYER_CONTROL_END,
)


@njit(cache=True)
def _edge(x0, y0, x1, y1, px, py):
    return (px - x0) * (y1 - y0) - (py - y0) * (x1 - x0)


@njit(cache=True)
def _fill_triangle(pixels, width, height, ax, ay, bx, by, cx, cy, color):
    area = _edge(ax, ay, bx, by, cx, cy)
    if area == 0:
        return
    left = max(min(ax, bx, cx), 0)
    right = min(max(ax, bx, cx), width - 1)
    top = max(min(ay, by, cy), 0)
    bottom = min(max(ay, by, cy), height - 1)
    if left > right or top > bottom:
        return
    sign = 1 if area > 0 else -1
    for y in range(top, bottom + 1):
        for x in range(left, right + 1):
            if (_edge(bx, by, cx, cy, x, y) * sign >= 0
                    and _edge(cx, cy, ax, ay, x, y) * sign >= 0
                    and _edge(ax, ay, bx, by, x, y) * sign >= 0):
                pixels[y * width + x] = color


def fill_triangle(buffer, width, height, ax, ay, bx, by, cx, cy, color):
    """Fill an indexed framebuffer in native code without copying its bytearray."""
    _fill_triangle(
        np.frombuffer(buffer, dtype=np.uint8), width, height,
        ax, ay, bx, by, cx, cy, color,
    )


@njit(cache=True)
def _rd16(mem, a):
    return (np.int64(mem[a & MEM_MASK]) << 8) | np.int64(mem[(a + 1) & MEM_MASK])


@njit(cache=True)
def _wr16(mem, a, v):
    mem[a & MEM_MASK] = (v >> 8) & 0xFF
    mem[(a + 1) & MEM_MASK] = v & 0xFF


@njit(cache=True)
def _wr_pc(mem, a, pc):
    mem[a & MEM_MASK] = (pc >> 24) & 0xFF
    mem[(a + 1) & MEM_MASK] = (pc >> 16) & 0xFF
    mem[(a + 2) & MEM_MASK] = (pc >> 8) & 0xFF
    mem[(a + 3) & MEM_MASK] = pc & 0xFF


@njit(cache=True)
def _rd_pc(mem, a):
    return (
        (np.int64(mem[a & MEM_MASK]) << 24)
        | (np.int64(mem[(a + 1) & MEM_MASK]) << 16)
        | (np.int64(mem[(a + 2) & MEM_MASK]) << 8)
        | np.int64(mem[(a + 3) & MEM_MASK])
    ) & MEM_MASK


@njit(cache=True)
def _hits_mmio(a, width=2):
    for offset in range(width):
        address = (a + offset) & MEM_MASK
        # Layer-register writes initialize GPU state through the Python MMIO path.
        if GPU_LAYER_CONTROL_START <= address < GPU_LAYER_CONTROL_END:
            return True
        if address in (
            GPU_BASE,
            DISK_BASE,
            CONSOLE_DATA,
            CONSOLE_STATUS,
            CONSOLE_CONTROL,
            MOUSE_EVENTS_ADDR,
        ):
            return True
    return False


@njit(cache=True)
def _condition(cond, z, c, n, v):
    if cond == 0:
        return z == 0
    if cond == 1:
        return c == 0
    if cond == 2:
        return n != 0
    if cond == 3:
        return n == 0
    if cond == 4:
        return v != 0
    if cond == 5:
        return v == 0
    if cond == 6:
        return n != v
    if cond == 7:
        return n == v
    if cond == 8:
        return z == 0 and n == v
    if cond == 9:
        return z != 0 or n != v
    if cond == 10:
        return c == 0 and z == 0
    if cond == 11:
        return c != 0 or z != 0
    return False


@njit(cache=True)
def run_block(mem, regs, st, budget):
    pc = st[0]
    sp = st[1]
    z = st[2]
    c = st[3]
    n = st[4]
    v = st[5]
    count = 0
    status = 0
    while count < budget:
        instr = _rd16(mem, pc)
        op = instr >> 11
        rd = (instr >> 8) & 7
        rs1 = (instr >> 5) & 7
        rs2 = (instr >> 2) & 7
        imm = instr & 0xFF
        npc = pc + 2
        bail = False

        if op == 31 and (instr & 0xF81F) == 0xF81B:
            bail = True
        elif op == 1:
            regs[rd] = imm
        elif op == 31 and (instr & 0xF8FF) == 0xF81E:
            control = _rd16(mem, npc)
            npc += 2
            if (control >> 12) in (8, 9):
                address = regs[control & 7]
                if _hits_mmio(address, 4):
                    bail = True
                elif (control >> 12) == 8:
                    low = _rd16(mem, address)
                    high = _rd16(mem, address + 2) & 0x3FFF
                    regs[rd] = (high << 16) | low
                else:
                    value = regs[rd] & MEM_MASK
                    _wr16(mem, address, value)
                    _wr16(mem, address + 2, value >> 16)
            elif (control >> 12) == 10:
                value = regs[rd] & MEM_MASK
                regs[rd] = value >> 16
                regs[control & 7] = value & 0xFFFF
            elif (control >> 12) == 11:
                regs[rd] = ((regs[rd] & 0x3FFF) << 16) | (regs[control & 7] & 0xFFFF)
            elif (control >> 12) == 7:
                high = _rd16(mem, npc) & 0x3FFF
                regs[rd] = ((high << 16) | _rd16(mem, npc + 2)) & MEM_MASK
                npc += 4
            else:
                if control & 0x0800:
                    right = _rd16(mem, npc)
                    npc += 2
                else:
                    right = regs[control & 7]
                left = regs[rd] & 0xFFFF
                right &= 0xFFFF
                sl = left - 0x10000 if left & 0x8000 else left
                sr = right - 0x10000 if right & 0x8000 else right
                oper = control >> 12
                carry = 0
                result = 0
                if oper == 0:
                    prod = left * right
                    result = prod & 0xFFFF
                    carry = 1 if prod > 0xFFFF else 0
                elif oper == 1:
                    result = (left * right) >> 16
                elif oper == 2:
                    result = ((sl * sr) >> 16) & 0xFFFF
                elif right == 0:
                    result = left if (oper == 5 or oper == 6) else 0
                    carry = 1
                elif oper == 3:
                    result = left // right
                elif oper == 5:
                    result = left % right
                else:
                    q = abs(sl) // abs(sr)
                    if (sl < 0) != (sr < 0):
                        q = -q
                    if oper == 4:
                        result = q & 0xFFFF
                    else:
                        result = (sl - q * sr) & 0xFFFF
                regs[rd] = result
                z = 1 if result == 0 else 0
                c = carry
        elif op == 31 and (instr & 0xF81F) == 0xF81A:
            control = _rd16(mem, npc)
            npc += 2
            oper = control >> 12
            src = control & 7
            if (rd & 1) or rd > 6 or (
                    not (6 <= oper <= 9) and ((src & 1) or src > 6)):
                bail = True
            else:
                left = ((regs[rd] & 0xFFFF) << 16) | (regs[rd + 1] & 0xFFFF)
                write = True
                result = 0
                if oper == 9:
                    result = (-left) & 0xFFFFFFFF
                    c = 1 if left != 0 else 0
                    v = 1 if left == 0x80000000 else 0
                    z = 1 if result == 0 else 0
                    n = result >> 31
                elif oper == 10:
                    regs[rd] = regs[src] & 0xFFFF
                    regs[rd + 1] = regs[src + 1] & 0xFFFF
                    write = False
                elif oper == 11 or oper == 12:
                    address = (((regs[src] & 0xFFFF) << 16)
                               | (regs[src + 1] & 0xFFFF)) & MEM_MASK
                    if _hits_mmio(address, 4):
                        bail = True
                    elif oper == 11:
                        regs[rd] = _rd16(mem, address)
                        regs[rd + 1] = _rd16(mem, address + 2)
                    else:
                        _wr16(mem, address, regs[rd])
                        _wr16(mem, address + 2, regs[rd + 1])
                    write = False
                elif oper >= 13:
                    right = ((regs[src] & 0xFFFF) << 16) | (regs[src + 1] & 0xFFFF)
                    if oper == 13:
                        result = (left * right) & 0xFFFFFFFF
                        c = 1 if left != 0 and right > 0xFFFFFFFF // left else 0
                        sl = left - 0x100000000 if left & 0x80000000 else left
                        sr = right - 0x100000000 if right & 0x80000000 else right
                        sp_ = sl * sr
                        v = 0 if -0x80000000 <= sp_ <= 0x7FFFFFFF else 1
                    else:
                        signed = (control & 0x0800) != 0
                        if right == 0:
                            result = left if oper == 15 else 0
                            c = 1
                        elif signed:
                            sl = left - 0x100000000 if left & 0x80000000 else left
                            sr = right - 0x100000000 if right & 0x80000000 else right
                            q = abs(sl) // abs(sr)
                            if (sl < 0) != (sr < 0):
                                q = -q
                            if oper == 14:
                                result = q & 0xFFFFFFFF
                            else:
                                result = (sl - q * sr) & 0xFFFFFFFF
                            c = 0
                        else:
                            result = left // right if oper == 14 else left % right
                            c = 0
                        v = 0
                    z = 1 if result == 0 else 0
                    n = result >> 31
                elif oper >= 6:
                    if control & 0x0800:
                        cnt = control & 0x1F
                    else:
                        cnt = regs[src] & 0x1F
                    if oper == 6:
                        if cnt:
                            c = (left >> (32 - cnt)) & 1
                        result = (left << cnt) & 0xFFFFFFFF
                    elif oper == 7:
                        if cnt:
                            c = (left >> (cnt - 1)) & 1
                        result = left >> cnt
                    else:
                        sv = left - 0x100000000 if left & 0x80000000 else left
                        if cnt:
                            c = (sv >> (cnt - 1)) & 1
                        result = (sv >> cnt) & 0xFFFFFFFF
                    z = 1 if result == 0 else 0
                    n = result >> 31
                    v = 0
                else:
                    right = ((regs[src] & 0xFFFF) << 16) | (regs[src + 1] & 0xFFFF)
                    if oper <= 2:
                        if oper == 0:
                            total = left + right
                            result = total & 0xFFFFFFFF
                            c = 1 if total > 0xFFFFFFFF else 0
                            v = 1 if (~(left ^ right) & (left ^ result) & 0x80000000) else 0
                        else:
                            total = left - right
                            result = total & 0xFFFFFFFF
                            c = 1 if total < 0 else 0
                            v = 1 if ((left ^ right) & (left ^ result) & 0x80000000) else 0
                        if oper == 2:
                            write = False
                    else:
                        if oper == 3:
                            result = left & right
                        elif oper == 4:
                            result = left | right
                        else:
                            result = left ^ right
                        c = 0
                        v = 0
                    z = 1 if result == 0 else 0
                    n = result >> 31
                if write and not bail:
                    regs[rd] = (result >> 16) & 0xFFFF
                    regs[rd + 1] = result & 0xFFFF
        elif op == 31 and (instr & 0xF8FF) == 0xF81F:
            control = _rd16(mem, npc)
            npc += 2
            oper = control >> 12
            left = regs[rd] & 0xFFFF
            if oper == 15:
                sel = control & 0x1F
                if sel == 0:
                    regs[rd] = ((left << 8) | (left >> 8)) & 0xFFFF
                elif sel == 1:
                    result = (left & 0xFF) | (0xFF00 if left & 0x80 else 0)
                    regs[rd] = result
                    z = 1 if (result & 0xFFFF) == 0 else 0
                    n = (result & 0xFFFF) >> 15
                    v = 0
                elif sel == 2:
                    npc = left
                elif sel == 3:
                    if _hits_mmio(sp - 4, 4):
                        bail = True
                    else:
                        sp -= 4
                        _wr_pc(mem, sp, npc)
                        npc = left
                elif sel == 4:
                    if _hits_mmio(sp - 2):
                        bail = True
                    else:
                        sp -= 2
                        _wr16(mem, sp, z | (c << 1) | (n << 2) | (v << 3))
                elif sel == 5:
                    if _hits_mmio(sp):
                        bail = True
                    else:
                        saved = _rd16(mem, sp)
                        sp += 2
                        z = saved & 1
                        c = (saved >> 1) & 1
                        n = (saved >> 2) & 1
                        v = (saved >> 3) & 1
                elif sel == 6:
                    c = 1
                elif sel == 7:
                    c = 0
                elif sel == 8:
                    sp = left
                elif sel == 9:
                    bits = 0
                    tmp = left
                    while tmp:
                        bits += 1
                        tmp >>= 1
                    regs[rd] = 16 - bits
                    z = 1 if left == 0 else 0
                elif sel == 10:
                    regs[rd] = (sp >> 16) & 0x3FFF
                elif sel == 11:
                    if _hits_mmio(sp - 4, 4):
                        bail = True
                    else:
                        high_register = (control >> 9) & 7
                        target = ((regs[high_register] & 0x3FFF) << 16) | left
                        sp -= 4
                        _wr_pc(mem, sp, npc)
                        npc = target
                elif sel == 12:
                    high_register = (control >> 9) & 7
                    npc = ((regs[high_register] & 0x3FFF) << 16) | left
                elif sel == 13:
                    sp = (sp & 0xFFFF) | ((left & 0x3FFF) << 16)
            elif oper <= 4:
                if control & 0x0800:
                    cnt = control & 0xF
                else:
                    cnt = regs[control & 7] & 0xF
                if cnt == 0:
                    result = left
                    carry = c
                elif oper == 0:
                    result = (left << cnt) & 0xFFFF
                    carry = (left >> (16 - cnt)) & 1
                elif oper == 1:
                    result = left >> cnt
                    carry = (left >> (cnt - 1)) & 1
                elif oper == 2:
                    signed = left - 0x10000 if left & 0x8000 else left
                    result = (signed >> cnt) & 0xFFFF
                    carry = (signed >> (cnt - 1)) & 1
                elif oper == 3:
                    result = ((left << cnt) | (left >> (16 - cnt))) & 0xFFFF
                    carry = result & 1
                else:
                    result = ((left >> cnt) | (left << (16 - cnt))) & 0xFFFF
                    carry = result >> 15
                regs[rd] = result
                z = 1 if result == 0 else 0
                n = result >> 15
                c = carry
                v = 0
            elif oper == 7:
                result = (-left) & 0xFFFF
                regs[rd] = result
                z = 1 if result == 0 else 0
                n = result >> 15
                c = 1 if left != 0 else 0
                v = 1 if left == 0x8000 else 0
            else:
                if control & 0x0800:
                    right = _rd16(mem, npc) & 0xFFFF
                    npc += 2
                else:
                    right = regs[control & 7] & 0xFFFF
                if oper == 5 or oper == 9:
                    total = left + right + (c if oper == 5 else 0)
                    result = total & 0xFFFF
                    v = 1 if ((left ^ result) & (right ^ result) & 0x8000) else 0
                    c = 1 if total > 0xFFFF else 0
                elif oper == 6 or oper == 10 or oper == 14:
                    total = left - right - (c if oper == 6 else 0)
                    result = total & 0xFFFF
                    v = 1 if ((left ^ right) & (left ^ result) & 0x8000) else 0
                    c = 1 if total < 0 else 0
                else:
                    if oper == 8 or oper == 11:
                        result = left & right
                    elif oper == 12:
                        result = left | right
                    else:
                        result = left ^ right
                    v = 0
                    c = 0
                z = 1 if result == 0 else 0
                n = result >> 15
                if oper != 8 and oper != 14:
                    regs[rd] = result
        elif op == 31 and (instr & FAR_CONTROL_MASK) == FAR_CONTROL_PREFIX:
            operation = (instr >> 9) & 3
            target = ((_rd16(mem, npc) & 0x3FFF) << 16) | _rd16(mem, npc + 2)
            npc += 4
            if operation == 1:
                if _hits_mmio(sp - 4, 4):
                    bail = True
                else:
                    sp -= 4
                    _wr_pc(mem, sp, npc)
                    npc = target
            elif operation == 0:
                npc = target
            elif operation == 2 and z:
                npc = target
            elif operation == 3 and c:
                npc = target
        elif op == 31 and (instr & FAR_BRANCH_MASK) == FAR_BRANCH_PREFIX:
            cond = (instr >> 7) & 0xF
            target = ((_rd16(mem, npc) & 0x3FFF) << 16) | _rd16(mem, npc + 2)
            npc += 4
            if _condition(cond, z, c, n, v):
                npc = target
        elif op == 31:
            if rs1 == 0:
                if (instr & 0xFF) == 0x1C:
                    regs[rd] = sp
                else:
                    regs[rd] = (regs[rd] & 0xFFFF) >> 1
            else:
                bail = True
        elif op == 27:
            addr = (sp + imm) & MEM_MASK
            if _hits_mmio(addr):
                bail = True
            else:
                regs[rd] = _rd16(mem, addr)
        elif op == 5:
            a = regs[rs1]
            b = regs[rs2]
            res = a + b
            regs[rd] = res
            z = 1 if res == 0 else 0
            c = 1 if res > 0xFF else 0
            r16 = res & 0xFFFF
            n = r16 >> 15
            v = 1 if ((a ^ r16) & (b ^ r16) & 0x8000) else 0
        elif op == 16:
            regs[rd] = (imm << 8) | regs[rd]
        elif op == 20:
            nsp = sp - 2
            if _hits_mmio(nsp):
                bail = True
            else:
                sp = nsp
                _wr16(mem, sp, regs[rd])
        elif op == 29:
            offset = imm if imm < 0x80 else imm - 0x100
            sp = (sp + offset) & MEM_MASK
        elif op == 4:
            regs[rd] = regs[rs1]
        elif op == 15:
            a = regs[rs1]
            b = regs[rs2]
            res = a - b
            z = 1 if res == 0 else 0
            c = 1 if res < 0 else 0
            r16 = res & 0xFFFF
            n = r16 >> 15
            v = 1 if ((a ^ b) & (a ^ r16) & 0x8000) else 0
        elif op == 23:
            if _hits_mmio(sp, 4):
                bail = True
            else:
                npc = _rd_pc(mem, sp)
                sp += 4
        elif op == 7:
            regs[rd] = regs[rs1] & regs[rs2]
        elif op == 25:
            addr = regs[rs1]
            if _hits_mmio(addr):
                bail = True
            else:
                regs[rd] = _rd16(mem, addr)
        elif op == 28:
            addr = (sp + imm) & MEM_MASK
            if _hits_mmio(addr):
                bail = True
            else:
                _wr16(mem, addr, regs[rd])
        elif op == 24:
            if z:
                npc = instr & 0x7FF
        elif op == 30:
            if c:
                npc = instr & 0x7FF
        elif op == 11:
            npc = instr & 0x7FF
        elif op == 2:
            addr = regs[rs1] & MEM_MASK
            if _hits_mmio(addr, 1):
                bail = True
            else:
                regs[rd] = np.int64(mem[addr])
        elif op == 3:
            a = regs[rs1]
            if _hits_mmio(a, 1):
                bail = True
            else:
                mem[a & MEM_MASK] = regs[rd] & 0xFF
        elif op == 26:
            a = regs[rs1]
            if _hits_mmio(a):
                bail = True
            else:
                _wr16(mem, a, regs[rd])
        elif op == 8:
            regs[rd] = regs[rs1] | regs[rs2]
        elif op == 6:
            a = regs[rs1]
            b = regs[rs2]
            res = a - b
            regs[rd] = res
            z = 1 if res == 0 else 0
            c = 1 if res < 0 else 0
            r16 = res & 0xFFFF
            n = r16 >> 15
            v = 1 if ((a ^ b) & (a ^ r16) & 0x8000) else 0
        elif op == 9:
            regs[rd] = regs[rs1] ^ regs[rs2]
        elif op == 13:
            regs[rd] = regs[rd] + 1
        elif op == 14:
            regs[rd] = regs[rd] - 1
        elif op == 12:
            if regs[rd] == 0:
                npc = imm
        elif op == 22:
            nsp = sp - 4
            if _hits_mmio(nsp, 4):
                bail = True
            else:
                sp = nsp
                _wr_pc(mem, sp, npc)
                npc = instr & 0x7FF
        elif op == 21:
            if _hits_mmio(sp):
                bail = True
            else:
                regs[rd] = _rd16(mem, sp) & 0xFFFF
                sp += 2
        elif op == 10:
            regs[rd] = (~regs[rs1]) & 0xFFFF
        elif op == 19:
            if _hits_mmio(sp, 6):
                bail = True
            else:
                saved = _rd16(mem, sp)
                sp += 2
                z = saved & 1
                c = (saved >> 1) & 1
                n = (saved >> 2) & 1
                v = (saved >> 3) & 1
                npc = _rd_pc(mem, sp)
                sp += 4
        elif op == 17:
            mem[ICR_ADDR] = mem[ICR_ADDR] | 1
        elif op == 18:
            mem[ICR_ADDR] = mem[ICR_ADDR] & 0xFE
        else:
            bail = True

        if bail:
            status = 1
            break
        pc = npc
        count += 1
        icr = mem[ICR_ADDR]
        if (icr & 1) and (icr & 0x0A):
            status = 2
            break

    st[0] = pc
    st[1] = sp
    st[2] = z
    st[3] = c
    st[4] = n
    st[5] = v
    return count, status
