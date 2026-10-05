"""Optional Numba-compiled CPU core for the SC-16 emulator.

`run_block` executes instructions exactly like `SuomiCompute16.step()`, but compiled to
machine code. It handles the instruction set (classic opcodes, extended ALU, MUL/DIV,
far branches and jumps) and stops ("bails out") *before* executing an instruction that
needs the Python emulator: a store to the graphics command register, HALT and the
half-precision float instructions. The caller then runs that one instruction with
`step()` and resumes. After an instruction that leaves an interrupt pending the loop
returns so Python can dispatch it.

State is passed in `regs` (8 registers) and `st` = [pc, sp, Z, C, N, V].
Returns (executed instruction count, status): 0 = budget used up,
1 = next instruction must be run by step(), 2 = check interrupts.
"""
import numpy as np
from numba import njit

# Must match the constants in SuomiCPU.py (checked by SuomiCPU before enabling the core).
MEM_MASK = 2**19 - 1
GPU_BASE = 0x43020
ICR_ADDR = 0x43002
CONSTANTS = (MEM_MASK, GPU_BASE, ICR_ADDR)


@njit(cache=True)
def _rd16(mem, a):
    return (np.int64(mem[a & MEM_MASK]) << 8) | np.int64(mem[(a + 1) & MEM_MASK])


@njit(cache=True)
def _wr16(mem, a, v):
    mem[a & MEM_MASK] = (v >> 8) & 0xFF
    mem[(a + 1) & MEM_MASK] = v & 0xFF


@njit(cache=True)
def _hits_gpu(a):
    return (a & MEM_MASK) == GPU_BASE or ((a + 1) & MEM_MASK) == GPU_BASE


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

        if op == 1:
            regs[rd] = imm
        elif op == 31 and (instr & 0xF8FF) == 0xF81E:
            control = _rd16(mem, npc)
            npc += 2
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
        elif op == 31 and (instr & 0xF8FF) == 0xF81F:
            control = _rd16(mem, npc)
            npc += 2
            oper = control >> 12
            left = regs[rd] & 0xFFFF
            if oper == 15:
                sel = control & 0xF
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
                    if _hits_gpu(sp - 2):
                        bail = True
                    else:
                        sp -= 2
                        _wr16(mem, sp, npc)
                        npc = left
                elif sel == 4:
                    if _hits_gpu(sp - 2):
                        bail = True
                    else:
                        sp -= 2
                        _wr16(mem, sp, z | (c << 1) | (n << 2) | (v << 3))
                elif sel == 5:
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
        elif op == 31 and (instr & 0xF81F) == 0xF81B:
            cond = rd | (rs1 << 3)
            target = _rd16(mem, npc) | (_rd16(mem, npc + 2) << 16)
            npc += 4
            if _condition(cond, z, c, n, v):
                npc = target
        elif op == 31 and (instr == 0xF81D or instr == 0xF91D or instr == 0xFA1D or instr == 0xFB1D):
            target = _rd16(mem, npc) | (_rd16(mem, npc + 2) << 16)
            npc += 4
            if instr == 0xF91D:
                if _hits_gpu(sp - 2):
                    bail = True
                else:
                    sp -= 2
                    _wr16(mem, sp, npc)
                    npc = target
            elif instr == 0xF81D:
                npc = target
            elif instr == 0xFA1D:
                if z:
                    npc = target
            elif c:
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
            regs[rd] = _rd16(mem, sp + imm)
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
            if _hits_gpu(nsp):
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
            npc = _rd16(mem, sp)
            sp += 2
        elif op == 7:
            regs[rd] = regs[rs1] & regs[rs2]
        elif op == 25:
            regs[rd] = _rd16(mem, regs[rs1])
        elif op == 28:
            addr = (sp + imm) & MEM_MASK
            if _hits_gpu(addr):
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
            regs[rd] = np.int64(mem[regs[rs1] & MEM_MASK])
        elif op == 3:
            a = regs[rs1]
            if (a & MEM_MASK) == GPU_BASE:
                bail = True
            else:
                mem[a & MEM_MASK] = regs[rd] & 0xFF
        elif op == 26:
            a = regs[rs1]
            if _hits_gpu(a):
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
            nsp = sp - 2
            if _hits_gpu(nsp):
                bail = True
            else:
                sp = nsp
                _wr16(mem, sp, npc)
                npc = instr & 0x7FF
        elif op == 21:
            regs[rd] = _rd16(mem, sp) & 0xFFFF
            sp += 2
        elif op == 10:
            regs[rd] = (~regs[rs1]) & 0xFFFF
        elif op == 19:
            saved = _rd16(mem, sp)
            sp += 2
            z = saved & 1
            c = (saved >> 1) & 1
            n = (saved >> 2) & 1
            v = (saved >> 3) & 1
            npc = _rd16(mem, sp)
            sp += 2
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
