use crate::emu::emu::Emu;
use crate::cpu::*;
use crate::util::util;

use super::inst_def::*;

fn rrc(emu: &mut Emu, value: u8) -> u8 {
    let carry_bit = value & 0x1;

    let val_full = (u16::from(value) >> 1) | (u16::from(carry_bit) << 7);

    let result = util::get_low(val_full);

    emu.cpu.set_flag(cpu::FLAG_Z, result == 0);
    emu.cpu.set_flag(cpu::FLAG_N, false);
    emu.cpu.set_flag(cpu::FLAG_H, false);
    emu.cpu.set_flag(cpu::FLAG_C, carry_bit == 0x1);

    return result;
}

fn rr(emu: &mut Emu, value: u8) -> u8 {
    let carry_flag: bool = emu.cpu.get_flag(cpu::FLAG_C);
    let carry_next = value & 0x1;

    let val_full = (u16::from(carry_flag) << 7) | (u16::from(value) >> 1);

    let result = util::get_low(val_full);

    emu.cpu.set_flag(cpu::FLAG_Z, result == 0);
    emu.cpu.set_flag(cpu::FLAG_N, false);
    emu.cpu.set_flag(cpu::FLAG_H, false);
    emu.cpu.set_flag(cpu::FLAG_C, carry_next == 0x1);

    return result;
}

fn rlc(emu: &mut Emu, value: u8) -> u8 {
    let carry_bit = value & 0x80;

    let val_full = (u16::from(value) << 1) | (u16::from(carry_bit) >> 7);

    let result = util::get_low(val_full);

    emu.cpu.set_flag(cpu::FLAG_Z, result == 0);
    emu.cpu.set_flag(cpu::FLAG_N, false);
    emu.cpu.set_flag(cpu::FLAG_H, false);
    emu.cpu.set_flag(cpu::FLAG_C, carry_bit == 0x80);

    return result;
}

fn rl(emu: &mut Emu, value: u8) -> u8 {
    let carry_flag: bool = emu.cpu.get_flag(cpu::FLAG_C);
    let carry_next = value & 0x80;

    let val_full = (u16::from(value) << 1) | u16::from(carry_flag);

    let result = util::get_low(val_full);

    emu.cpu.set_flag(cpu::FLAG_Z, result == 0);
    emu.cpu.set_flag(cpu::FLAG_N, false);
    emu.cpu.set_flag(cpu::FLAG_H, false);
    emu.cpu.set_flag(cpu::FLAG_C, carry_next == 0x80);

    return result;
}

fn consume_src_r8_imm8_hladdr(emu: &mut Emu, src: OperandKind, opcode: u8) -> u8 {
    let val = match src {
        OperandKind::R8 => {
            let src_reg = opcode & 0x7;
            debug_assert!(src_reg != 0x6);
            debug_assert!(src_reg < 0x8);
            cpu::CPU::read_r8(emu, src_reg)
        }
        OperandKind::R16_Addr => {
            emu.bus_read(emu.cpu.hl)
        }
        OperandKind::Imm8 => {
            let val = emu.bus_read(emu.cpu.pc);
            emu.cpu.pc += 1;
            val
        }
        _ => unreachable!(),
    };
    val
}

fn add_a8(emu: &mut Emu, val: u8, carry: u8) {
    let dst_val = util::get_high(emu.cpu.af);

    let sum_full = u16::from(dst_val) + u16::from(val) + u16::from(carry);
    let sum_low = util::get_low(sum_full);
    
    emu.cpu.set_flag(cpu::FLAG_Z, sum_low == 0);
    emu.cpu.set_flag(cpu::FLAG_N, false);
    emu.cpu.set_flag(cpu::FLAG_H, ((dst_val & 0xF) + (val & 0xF) + carry) > 0xF);
    emu.cpu.set_flag(cpu::FLAG_C, sum_full > 0xFF);

    util::set_high(&mut emu.cpu.af, sum_low);
}

fn sub_a8(emu: &mut Emu, val: u8, carry: u8) {
    let dst_val = util::get_high(emu.cpu.af);

    let res_full = i16::from(dst_val) - i16::from(val) - i16::from(carry);
    let res_low = util::get_low(res_full as u16);

    let half_carry = ((dst_val as i8) & 0xF) - ((val as i8) & 0xF) - (carry as i8);
    
    emu.cpu.set_flag(cpu::FLAG_Z, res_low == 0);
    emu.cpu.set_flag(cpu::FLAG_N, true);
    emu.cpu.set_flag(cpu::FLAG_H, half_carry < 0);
    emu.cpu.set_flag(cpu::FLAG_C, res_full < 0);

    util::set_high(&mut emu.cpu.af, res_low);
}

fn and_a8(emu: &mut Emu, val: u8) {
    let result = util::get_high(emu.cpu.af) & val;

    emu.cpu.set_flag(cpu::FLAG_Z, result == 0);
    emu.cpu.set_flag(cpu::FLAG_N, false);
    emu.cpu.set_flag(cpu::FLAG_H, true);
    emu.cpu.set_flag(cpu::FLAG_C, false);

    util::set_high(&mut emu.cpu.af, result);
}

fn xor_a8(emu: &mut Emu, val: u8) {
    let result = util::get_high(emu.cpu.af) ^ val;

    emu.cpu.set_flag(cpu::FLAG_Z, result == 0);
    emu.cpu.set_flag(cpu::FLAG_N, false);
    emu.cpu.set_flag(cpu::FLAG_H, false);
    emu.cpu.set_flag(cpu::FLAG_C, false);

    util::set_high(&mut emu.cpu.af, result);
}

fn or_a8(emu: &mut Emu, val: u8) {
    let result = util::get_high(emu.cpu.af) | val;

    emu.cpu.set_flag(cpu::FLAG_Z, result == 0);
    emu.cpu.set_flag(cpu::FLAG_N, false);
    emu.cpu.set_flag(cpu::FLAG_H, false);
    emu.cpu.set_flag(cpu::FLAG_C, false);

    util::set_high(&mut emu.cpu.af, result);
}

pub fn opcode_nop(emu: &mut Emu, instr: &Instruction, opcode: u8) { }

pub fn opcode_ld(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    match (instr.dst, instr.src) {
        (OperandKind::R8, OperandKind::R8) => {
            // 0b01xxxyyy
            let src_reg = opcode & 0x7;
            let dst_reg = (opcode >> 3) & 0x7;
            let val = cpu::CPU::read_r8(emu, src_reg);

            // note: reg2reg will never trigger 0x6 write to [hl]
            debug_assert!(dst_reg != 0x6);

            cpu::CPU::write_r8(emu, dst_reg, val);
        }
        (OperandKind::R8, OperandKind::Imm8) => {
            let dst_reg = (opcode >> 3) & 0x7;
            let n8 = emu.bus_read(emu.cpu.pc);
            emu.cpu.pc += 1;

            debug_assert!(dst_reg != 0x6); // should only happen with 0x36
            cpu::CPU::write_r8(emu, dst_reg, n8);
        }
        (OperandKind::R8, OperandKind::R16_Addr) => {
            match opcode {
                0x0A /* LD A [BC] */ => {
                    let val = emu.bus_read(emu.cpu.bc);
                    util::set_high(&mut emu.cpu.af, val);
                }
                0x1A /* LD A [DE] */ => {
                    let val = emu.bus_read(emu.cpu.de);
                    util::set_high(&mut emu.cpu.af, val);
                }
                0x2A /* LD A [HL+] */ => {
                    let val = emu.bus_read(emu.cpu.hl);
                    util::set_high(&mut emu.cpu.af, val);
                    emu.cpu.hl += 1;
                }
                0x3A /* LD A [HL-] */ => {
                    let val = emu.bus_read(emu.cpu.hl);
                    util::set_high(&mut emu.cpu.af, val);
                    emu.cpu.hl -= 1;
                }
                _ => /* LD r8, [HL] */ {
                    debug_assert!([0x46, 0x4E, 0x56, 0x5E, 0x66, 0x6E, 0x7E].contains(&opcode));
                    
                    let src_reg = opcode & 0x7;

                    // src_reg is always 0x6 e.g [HL]
                    debug_assert!(src_reg == 0x6);

                    let dst_reg = (opcode >> 3) & 0x7;
                    let val = cpu::CPU::read_r8(emu, src_reg);
        
                    // note: r16addr2r8 will never trigger 0x6 write to [hl]
                    debug_assert!(dst_reg != 0x6);
        
                    cpu::CPU::write_r8(emu, dst_reg, val);
                }
            }
        }
        (OperandKind::R8, OperandKind::Imm16_Addr) => {
            debug_assert!(opcode == 0xFA /* LD A [a16] */);
            let lsb = emu.bus_read(emu.cpu.pc);
            let msb = emu.bus_read(emu.cpu.pc + 1);
            emu.cpu.pc += 2;
            let val = emu.bus_read(util::value(msb, lsb));
            util::set_high(&mut emu.cpu.af, val);
        }
        (OperandKind::R16, OperandKind::R16) => {
            debug_assert!([0xF8, 0xF9].contains(&opcode));
            match opcode {
                0xF8 /* LD HL SP+ */ => {
                    let e: i8 = util::consume_signed_from_pc(emu);

                    let sum = emu.cpu.sp.wrapping_add_signed(e.into());
                    emu.cpu.hl = sum;

                    emu.cpu.set_flag(cpu::FLAG_Z, false);
                    emu.cpu.set_flag(cpu::FLAG_N, false);
                    emu.cpu.set_flag(cpu::FLAG_H, ((emu.cpu.sp ^ u16::from(e as u8) ^ sum) & 0x10) == 0x10);
                    emu.cpu.set_flag(cpu::FLAG_C, ((emu.cpu.sp ^ u16::from(e as u8) ^ sum) & 0x100) == 0x100);
                }
                0xF9 /* LD SP HL  */=> {
                    emu.cpu.sp = emu.cpu.hl;
                }
                _ => unreachable!(),
            }
        }
        (OperandKind::R16, OperandKind::Imm16) => {
            debug_assert!([0x01, 0x11, 0x21, 0x31].contains(&opcode));
            let dst_reg = (opcode >> 4) & 0x3;

            let lsb = emu.bus_read(emu.cpu.pc);
            let msb = emu.bus_read(emu.cpu.pc + 1);
            emu.cpu.pc += 2;

            cpu::CPU::write_r16(emu, dst_reg, util::value(msb, lsb));
        }
        (OperandKind::R16_Addr, OperandKind::R8) => {
            match opcode {
                0x22 /* LD [HL+] A */ => {
                    emu.bus_write(emu.cpu.hl, util::get_high(emu.cpu.af));
                    emu.cpu.hl += 1;
                }
                0x32 /* LD [HL-] A */ => {
                    emu.bus_write(emu.cpu.hl, util::get_high(emu.cpu.af));
                    emu.cpu.hl -= 1;
                }
                0x02 /* LD [BC] A */ => {
                    emu.bus_write(emu.cpu.bc, util::get_high(emu.cpu.af));
                }
                0x12 /* LD [DE] A */ => {
                    emu.bus_write(emu.cpu.de, util::get_high(emu.cpu.af));
                }
                /* LD [HL], r8 */ _ => {
                    debug_assert!([0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x77].contains(&opcode));

                    let src_reg = opcode & 0x7;
                    let val = cpu::CPU::read_r8(emu, src_reg);

                    // dst_reg will always be 0x6
                    debug_assert!((opcode >> 3) & 0x7 == 0x6);
                    emu.bus_write(emu.cpu.hl, val);
                }
            }
        }
        (OperandKind::Imm16_Addr, OperandKind::R16) => {
            debug_assert!(opcode == 0x08);

            let lsb = emu.bus_read(emu.cpu.pc);
            let msb = emu.bus_read(emu.cpu.pc + 1);
            emu.cpu.pc += 2;

            let addr = util::value(msb, lsb);
            emu.bus_write(addr, util::get_low(emu.cpu.sp));
            emu.bus_write(addr + 1, util::get_high(emu.cpu.sp));
        }
        (OperandKind::R16_Addr, OperandKind::Imm8) => {
            debug_assert!(opcode == 0x36); // could technically be decoded same as OperandKind::R8, OperandKind::Imm8
            let val = emu.bus_read(emu.cpu.pc);
            emu.cpu.pc += 1;
            cpu::CPU::write_r8(emu, 0x6, val);
        }
        (OperandKind::R8_Addr, OperandKind::R8) => {
            debug_assert!(opcode == 0xE2);
            let addr = u16::from(util::get_low(emu.cpu.bc)) | 0xFF00;
            emu.bus_write(addr, util::get_high(emu.cpu.af));
        }
        (OperandKind::Imm16_Addr, OperandKind::R8) => {
            debug_assert!(opcode == 0xEA);

            let lsb = emu.bus_read(emu.cpu.pc);
            let msb = emu.bus_read(emu.cpu.pc + 1);
            emu.cpu.pc += 2;

            let addr = util::value(msb, lsb);
            emu.bus_write(addr, util::get_high(emu.cpu.af));
        }
        _ => unreachable!()
    }
}

pub fn opcode_inc(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    match instr.dst {
        OperandKind::R8 => {
            let dst_reg = (opcode >> 3) & 0x7;
            debug_assert!(dst_reg != 0x6);

            let curr_val = cpu::CPU::read_r8(emu, dst_reg);

            let sum = curr_val.wrapping_add(1);
            emu.cpu.set_flag(cpu::FLAG_Z, sum == 0);
            emu.cpu.set_flag(cpu::FLAG_N, false);
            emu.cpu.set_flag(cpu::FLAG_H, (sum & 0xF) == 0);

            cpu::CPU::write_r8(emu, dst_reg, sum);
        }
        OperandKind::R16 => {
            let dst_reg = (opcode >> 4) & 0x3;
            let curr_val = cpu::CPU::read_r16(emu, dst_reg);
            let sum = curr_val.wrapping_add(1);
            cpu::CPU::write_r16(emu, dst_reg, sum);
        }
        OperandKind::R16_Addr => {
            debug_assert!(opcode == 0x34);

            let curr_val = emu.bus_read(emu.cpu.hl);

            let sum = curr_val.wrapping_add(1);
            emu.cpu.set_flag(cpu::FLAG_Z, sum == 0);
            emu.cpu.set_flag(cpu::FLAG_N, false);
            emu.cpu.set_flag(cpu::FLAG_H, (sum & 0xF) == 0);

            emu.bus_write(emu.cpu.hl, sum);
        }
        _ => unreachable!(),
    }
}

pub fn opcode_dec(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    match instr.dst {
        OperandKind::R8 => {
            let dst_reg = (opcode >> 3) & 0x7;
            debug_assert!(dst_reg != 0x6);

            let curr_val = cpu::CPU::read_r8(emu, dst_reg);

            let sum = curr_val.wrapping_sub(1);
            emu.cpu.set_flag(cpu::FLAG_Z, sum == 0);
            emu.cpu.set_flag(cpu::FLAG_N, true);
            emu.cpu.set_flag(cpu::FLAG_H, (sum & 0x0F) == 0x0F);

            cpu::CPU::write_r8(emu, dst_reg, sum);
        }
        OperandKind::R16 => {
            let dst_reg = (opcode >> 4) & 0x3;
            let curr_val = cpu::CPU::read_r16(emu, dst_reg);
            let sum = curr_val.wrapping_sub(1);
            cpu::CPU::write_r16(emu, dst_reg, sum);
        }
        OperandKind::R16_Addr => {
            debug_assert!(opcode == 0x35);

            let curr_val = emu.bus_read(emu.cpu.hl);

            let sum = curr_val.wrapping_sub(1);
            emu.cpu.set_flag(cpu::FLAG_Z, sum == 0);
            emu.cpu.set_flag(cpu::FLAG_N, true);
            emu.cpu.set_flag(cpu::FLAG_H, (sum & 0x0F) == 0x0F);

            emu.bus_write(emu.cpu.hl, sum);
        }
        _ => unreachable!(),
    }
}

pub fn opcode_rlca(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    debug_assert!(opcode == 0x7);

    let result = rlc(emu, util::get_high(emu.cpu.af));

    util::set_high(&mut emu.cpu.af, result);
    emu.cpu.set_flag(cpu::FLAG_Z, false);
}

pub fn opcode_rrca(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    debug_assert!(opcode == 0x0F);

    let result = rrc(emu, util::get_high(emu.cpu.af));

    util::set_high(&mut emu.cpu.af, result);
    emu.cpu.set_flag(cpu::FLAG_Z, false);
}

pub fn opcode_rla(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    debug_assert!(opcode == 0x17);

    let result = rl(emu, util::get_high(emu.cpu.af));

    util::set_high(&mut emu.cpu.af, result);
    emu.cpu.set_flag(cpu::FLAG_Z, false);
}

pub fn opcode_rra(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    debug_assert!(opcode == 0x1F);

    let result = rr(emu, util::get_high(emu.cpu.af));

    util::set_high(&mut emu.cpu.af, result);
    emu.cpu.set_flag(cpu::FLAG_Z, false);
}

pub fn opcode_add(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    match (instr.dst, instr.src) {
        (OperandKind::R8, OperandKind::R8 | OperandKind::R16_Addr) => {
            // ADD A, r8
            // ADD A, [HL]
            let src_reg_or_hladdr = opcode & 0x7;
            let src_val = cpu::CPU::read_r8(emu, src_reg_or_hladdr);

            add_a8(emu, src_val, 0);
        }
        (OperandKind::R16, OperandKind::R16) => {
            // ADD HL r16
            let src_reg = (opcode >> 4) & 0x3;
            let src_val = cpu::CPU::read_r16(emu, src_reg);
            let dst_val = emu.cpu.hl;

            let sum: u32 = u32::from(src_val) + u32::from(dst_val);

            emu.cpu.set_flag(cpu::FLAG_N, false);
            emu.cpu.set_flag(cpu::FLAG_H, (dst_val & 0xFFF) + (src_val & 0xFFF) > 0xFFF);
            emu.cpu.set_flag(cpu::FLAG_C, sum > 0xFFFF);

            emu.cpu.hl = (sum & 0xFFFF) as u16;
        }
        (OperandKind::R8, OperandKind::Imm8) => {
            debug_assert!(opcode == 0xC6);
            let src_val = emu.bus_read(emu.cpu.pc);
            emu.cpu.pc += 1;

            add_a8(emu, src_val, 0);
        }
        (OperandKind::R16, OperandKind::Imm8) => {
            // ADD SP e8
            debug_assert!(opcode == 0xE8);

            let e: i8 = util::consume_signed_from_pc(emu);
            let sum = emu.cpu.sp.wrapping_add_signed(e.into());

            emu.cpu.set_flag(cpu::FLAG_Z, false);
            emu.cpu.set_flag(cpu::FLAG_N, false);
            emu.cpu.set_flag(cpu::FLAG_H, ((emu.cpu.sp ^ u16::from(e as u8) ^ sum) & 0x10) == 0x10);
            emu.cpu.set_flag(cpu::FLAG_C, ((emu.cpu.sp ^ u16::from(e as u8) ^ sum) & 0x100) == 0x100);

            emu.cpu.sp = sum;
        }
        _ => unreachable!(),
    }
}

pub fn opcode_stop(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    debug_assert!(opcode == 0x10);
    // Note: Enter CPU very low power mode. Also used to switch between double and normal speed CPU modes in GBC.
}

pub fn opcode_jr(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    let branch_taken = match opcode {
        0x18 /* JR e8 */ => {
            true
        }
        _ => {
            debug_assert!([0x20, 0x28, 0x30, 0x38].contains(&opcode));
            let flag = (opcode >> 3) & 0x3;

            let cond = match flag {
                0x3 => emu.cpu.get_flag(cpu::FLAG_C),
                0x2 => !emu.cpu.get_flag(cpu::FLAG_C),
                0x1 => emu.cpu.get_flag(cpu::FLAG_Z),
                0x0 => !emu.cpu.get_flag(cpu::FLAG_Z),
                _ => unreachable!(),
            };

            cond
        }
    };

    if branch_taken {
        let e: i8 = util::consume_signed_from_pc(emu);
        emu.cpu.pc = emu.cpu.pc.wrapping_add_signed(e.into());
    } else {
        emu.cpu.jmp_skipped = true;
        emu.cpu.pc += 1;
    }
}

pub fn opcode_daa(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    let original_val = util::get_high(emu.cpu.af);
    let flag_h = emu.cpu.get_flag(cpu::FLAG_H);
    let flag_c = emu.cpu.get_flag(cpu::FLAG_C);
    let flag_n = emu.cpu.get_flag(cpu::FLAG_N);

    let mut offset: u8 = 0;
    let mut carry_next = false;

    if (!flag_n && (original_val & 0xF) > 0x9) || flag_h {
        offset |= 0x6;
    }

    if (!flag_n && original_val > 0x99) || flag_c {
        offset |= 0x60;
        carry_next = true;
    }

    let daa_value = if flag_n {
        original_val.wrapping_sub(offset)
    } else {
        original_val.wrapping_add(offset)
    };

    util::set_high(&mut emu.cpu.af, daa_value);
    emu.cpu.set_flag(cpu::FLAG_C, carry_next);
    emu.cpu.set_flag(cpu::FLAG_H, false);
    emu.cpu.set_flag(cpu::FLAG_Z, daa_value == 0);
}

pub fn opcode_cpl(emu: &mut Emu, _instr: &Instruction, _opcode: u8) {
    let val = !util::get_high(emu.cpu.af);
    util::set_high(&mut emu.cpu.af, val);
    emu.cpu.set_flag(cpu::FLAG_N, true);
    emu.cpu.set_flag(cpu::FLAG_H, true);
}

pub fn opcode_scf(emu: &mut Emu, _instr: &Instruction, _opcode: u8) {
    emu.cpu.set_flag(cpu::FLAG_C, true);
    emu.cpu.set_flag(cpu::FLAG_N, false);
    emu.cpu.set_flag(cpu::FLAG_H, false);
}

pub fn opcode_ccf(emu: &mut Emu, _instr: &Instruction, _opcode: u8) {
    emu.cpu.set_flag(cpu::FLAG_C, !emu.cpu.get_flag(cpu::FLAG_C));
    emu.cpu.set_flag(cpu::FLAG_N, false);
    emu.cpu.set_flag(cpu::FLAG_H, false);
}

pub fn opcode_halt(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0x76"); }

pub fn opcode_adc(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    let src_val = consume_src_r8_imm8_hladdr(emu, instr.src, opcode);
    add_a8(emu, src_val, emu.cpu.get_flag(cpu::FLAG_C).into());
}

pub fn opcode_sub(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    let src_val = consume_src_r8_imm8_hladdr(emu, instr.src, opcode);
    sub_a8(emu, src_val, 0);
}

pub fn opcode_sbc(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    let src_val = consume_src_r8_imm8_hladdr(emu, instr.src, opcode);
    sub_a8(emu, src_val, emu.cpu.get_flag(cpu::FLAG_C).into());
}

pub fn opcode_and(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    let src_val = consume_src_r8_imm8_hladdr(emu, instr.src, opcode);
    and_a8(emu, src_val);
}

pub fn opcode_xor(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    let src_val = consume_src_r8_imm8_hladdr(emu, instr.src, opcode);
    xor_a8(emu, src_val);
}

pub fn opcode_or(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    let src_val = consume_src_r8_imm8_hladdr(emu, instr.src, opcode);
    or_a8(emu, src_val);
}

pub fn opcode_cp(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xFE"); }

pub fn opcode_ret(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    println!("{}", emu.cpu);
    todo!("0xC0, 0xC8, 0xC9, 0xD0, 0xD8");
}

pub fn opcode_pop(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xC1, 0xD1, 0xE1, 0xF1"); }

pub fn opcode_jp(emu: &mut Emu, instr: &Instruction, opcode: u8) {
    if opcode == 0xE9 /* JP HL */ {
        emu.cpu.pc = emu.cpu.hl;
        return;
    }

    let branch_taken = match opcode {
        0xC3 /* JP a16 */ => {
            true
        }
        _ => {
            let flag = (opcode >> 3) & 0x3;

            let cond = match flag {
                0x3 => emu.cpu.get_flag(cpu::FLAG_C),
                0x2 => !emu.cpu.get_flag(cpu::FLAG_C),
                0x1 => emu.cpu.get_flag(cpu::FLAG_Z),
                0x0 => !emu.cpu.get_flag(cpu::FLAG_Z),
                _ => unreachable!(),
            };

            cond
        }
    };

    if branch_taken {
        let low = emu.bus_read(emu.cpu.pc);
        let high = emu.bus_read(emu.cpu.pc + 1);
        emu.cpu.pc = util::value(high, low);
    } else {
        emu.cpu.jmp_skipped = true;
        emu.cpu.pc += 2;
    }
}

pub fn opcode_call(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xC4, 0xCC, 0xCD, 0xD4, 0xDC"); }
pub fn opcode_push(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xC5, 0xD5, 0xE5, 0xF5"); }
pub fn opcode_rst(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xC7, 0xCF, 0xD7, 0xDF, 0xE7, 0xEF, 0xF7, 0xFF"); }
pub fn opcode_prefix(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xCB"); }
pub fn opcode_illegal_d3(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xD3"); }
pub fn opcode_reti(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xD9"); }
pub fn opcode_illegal_db(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xDB"); }
pub fn opcode_illegal_dd(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xDD"); }
pub fn opcode_ldh(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xE0, 0xF0"); }
pub fn opcode_illegal_e3(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xE3"); }
pub fn opcode_illegal_e4(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xE4"); }
pub fn opcode_illegal_eb(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xEB"); }
pub fn opcode_illegal_ec(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xEC"); }
pub fn opcode_illegal_ed(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xED"); }
pub fn opcode_di(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xF3"); }
pub fn opcode_illegal_f4(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xF4"); }
pub fn opcode_ei(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xFB"); }
pub fn opcode_illegal_fc(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xFC"); }
pub fn opcode_illegal_fd(emu: &mut Emu, instr: &Instruction, opcode: u8) { todo!("0xFD"); }