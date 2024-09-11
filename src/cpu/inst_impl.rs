use crate::cpu::*;
use crate::mmu::mmu::MMU;
use crate::util::util;

use super::inst_def::*;

impl cpu::CPU {
    pub fn opcode_nop(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { }

    pub fn opcode_ld(&mut self, mmu: &mut MMU, instr: &Instruction, opcode: u8) {
        match (instr.dst, instr.src) {
            (OperandKind::R8, OperandKind::R8) => {
                // 0b01xxxyyy
                let src_reg = opcode & 0x7;
                let dst_reg = (opcode >> 3) & 0x7;
                let val = self.read_r8(mmu, src_reg);

                // note: reg2reg will never trigger 0x6 write to [hl]
                debug_assert!(dst_reg != 0x6);

                self.write_r8(mmu, dst_reg, val);
            }
            (OperandKind::R8, OperandKind::Imm8) => {
                let dst_reg = (opcode >> 3) & 0x7;
                let n8 = self.consume_byte_from_pc(mmu);

                debug_assert!(dst_reg != 0x6); // should only happen with 0x36
                self.write_r8(mmu, dst_reg, n8);
            }
            (OperandKind::R8, OperandKind::R16Addr) => {
                match opcode {
                    0x0A /* LD A [BC] */ => {
                        let val = mmu.bus_read(self.bc().get());
                        self.a().set(val);
                    }
                    0x1A /* LD A [DE] */ => {
                        let val = mmu.bus_read(self.de().get());
                        self.a().set(val);
                    }
                    0x2A /* LD A [HL+] */ => {
                        let val = mmu.bus_read(self.hl().get());
                        self.a().set(val);
                        self.hl().inc();
                    }
                    0x3A /* LD A [HL-] */ => {
                        let val = mmu.bus_read(self.hl().get());
                        self.a().set(val);
                        self.hl().dec();
                    }
                    _ => /* LD r8, [HL] */ {
                        debug_assert!([0x46, 0x4E, 0x56, 0x5E, 0x66, 0x6E, 0x7E].contains(&opcode));
                        
                        let src_reg = opcode & 0x7;

                        // src_reg is always 0x6 e.g [HL]
                        debug_assert!(src_reg == 0x6);

                        let dst_reg = (opcode >> 3) & 0x7;
                        let val = self.read_r8(mmu, src_reg);
            
                        // note: r16addr2r8 will never trigger 0x6 write to [hl]
                        debug_assert!(dst_reg != 0x6);
            
                        self.write_r8(mmu, dst_reg, val);
                    }
                }
            }
            (OperandKind::R8, OperandKind::Imm16Addr) => {
                debug_assert!(opcode == 0xFA /* LD A [a16] */);
                let lsb = self.consume_byte_from_pc(mmu);
                let msb = self.consume_byte_from_pc(mmu);
                let val = mmu.bus_read(util::value(msb, lsb));
                self.a().set(val);
            }
            (OperandKind::R16, OperandKind::R16) => {
                debug_assert!([0xF8, 0xF9].contains(&opcode));
                match opcode {
                    0xF8 /* LD HL SP+ */ => {
                        let e = self.consume_byte_from_pc(mmu) as i8;

                        let sp_val = self.sp().get();
                        let sum = i32::from(sp_val) + i32::from(e);

                        self.set_flag(cpu::FLAG_Z, false);
                        self.set_flag(cpu::FLAG_N, false);
                        self.set_flag(cpu::FLAG_H, (i32::from(sp_val) ^ i32::from(e) ^ (i32::from(sum) & 0xFFFF)) & 0x10 == 0x10);
                        self.set_flag(cpu::FLAG_C, (i32::from(sp_val) ^ i32::from(e) ^ (i32::from(sum) & 0xFFFF)) & 0x100 == 0x100);

                        self.hl().set(sum as u16);
                    }
                    0xF9 /* LD SP HL  */=> {
                        let hl_val = self.hl().get();
                        self.sp().set(hl_val);
                    }
                    _ => unreachable!(),
                }
            }
            (OperandKind::R16, OperandKind::Imm16) => {
                debug_assert!([0x01, 0x11, 0x21, 0x31].contains(&opcode));
                let dst_reg = (opcode >> 4) & 0x3;

                let lsb = self.consume_byte_from_pc(mmu);
                let msb = self.consume_byte_from_pc(mmu);

                self.write_r16(dst_reg, util::value(msb, lsb));
            }
            (OperandKind::R16Addr, OperandKind::R8) => {
                match opcode {
                    0x22 /* LD [HL+] A */ => {
                        mmu.bus_write(self.hl().inc(), self.a().get());
                    }
                    0x32 /* LD [HL-] A */ => {
                        mmu.bus_write(self.hl().dec(), self.a().get());
                    }
                    0x02 /* LD [BC] A */ => {
                        mmu.bus_write(self.bc().get(), self.a().get());
                    }
                    0x12 /* LD [DE] A */ => {
                        mmu.bus_write(self.de().get(), self.a().get());
                    }
                    /* LD [HL], r8 */ _ => {
                        debug_assert!([0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x77].contains(&opcode));

                        let src_reg = opcode & 0x7;
                        let val = self.read_r8(mmu, src_reg);

                        // dst_reg will always be 0x6
                        debug_assert!((opcode >> 3) & 0x7 == 0x6);
                        mmu.bus_write(self.hl().get(), val);
                    }
                }
            }
            (OperandKind::Imm16Addr, OperandKind::R16) => {
                debug_assert!(opcode == 0x08);

                let lsb = self.consume_byte_from_pc(mmu);
                let msb = self.consume_byte_from_pc(mmu);

                let addr = util::value(msb, lsb);

                let sp_lsb = util::get_low(self.sp().get());
                let sp_msb = util::get_high(self.sp().get());

                mmu.bus_write(addr, sp_lsb);
                mmu.bus_write(addr + 1, sp_msb);
            }
            (OperandKind::R16Addr, OperandKind::Imm8) => {
                debug_assert!(opcode == 0x36); // could technically be decoded same as OperandKind::R8, OperandKind::Imm8
                let val = self.consume_byte_from_pc(mmu);
                self.write_r8(mmu, 0x6, val);
            }
            (OperandKind::R8Addr, OperandKind::R8) => {
                debug_assert!(opcode == 0xE2);
                let addr = u16::from(self.c().get()) | 0xFF00;
                mmu.bus_write(addr, self.a().get());
            }
            (OperandKind::Imm16Addr, OperandKind::R8) => {
                debug_assert!(opcode == 0xEA);

                let lsb = self.consume_byte_from_pc(mmu);
                let msb = self.consume_byte_from_pc(mmu);

                let addr = util::value(msb, lsb);
                mmu.bus_write(addr, self.a().get());
            }
            _ => unreachable!()
        }
    }

    pub fn opcode_inc(&mut self, mmu: &mut MMU, instr: &Instruction, opcode: u8) {
        match instr.dst {
            OperandKind::R8 => {
                let dst_reg = (opcode >> 3) & 0x7;
                debug_assert!(dst_reg != 0x6);

                let curr_val = self.read_r8(mmu, dst_reg);

                let sum = curr_val.wrapping_add(1);
                self.set_flag(cpu::FLAG_Z, sum == 0);
                self.set_flag(cpu::FLAG_N, false);
                self.set_flag(cpu::FLAG_H, (sum & 0xF) == 0);

                self.write_r8(mmu, dst_reg, sum);
            }
            OperandKind::R16 => {
                let dst_reg = (opcode >> 4) & 0x3;
                let curr_val = self.read_r16(dst_reg);
                let sum = curr_val.wrapping_add(1);
                self.write_r16(dst_reg, sum);
            }
            OperandKind::R16Addr => {
                debug_assert!(opcode == 0x34);

                let curr_val = mmu.bus_read(self.hl().get());

                let sum = curr_val.wrapping_add(1);
                self.set_flag(cpu::FLAG_Z, sum == 0);
                self.set_flag(cpu::FLAG_N, false);
                self.set_flag(cpu::FLAG_H, (sum & 0xF) == 0);

                mmu.bus_write(self.hl().get(), sum);
            }
            _ => unreachable!(),
        }
    }

    pub fn opcode_dec(&mut self, mmu: &mut MMU, instr: &Instruction, opcode: u8) {
        match instr.dst {
            OperandKind::R8 => {
                let dst_reg = (opcode >> 3) & 0x7;
                debug_assert!(dst_reg != 0x6);

                let curr_val = self.read_r8(mmu, dst_reg);

                let sum = curr_val.wrapping_sub(1);
                self.set_flag(cpu::FLAG_Z, sum == 0);
                self.set_flag(cpu::FLAG_N, true);
                self.set_flag(cpu::FLAG_H, (sum & 0x0F) == 0x0F);

                self.write_r8(mmu, dst_reg, sum);
            }
            OperandKind::R16 => {
                let dst_reg = (opcode >> 4) & 0x3;
                let curr_val = self.read_r16(dst_reg);
                let sum = curr_val.wrapping_sub(1);
                self.write_r16(dst_reg, sum);
            }
            OperandKind::R16Addr => {
                debug_assert!(opcode == 0x35);

                let curr_val = mmu.bus_read(self.hl().get());

                let sum = curr_val.wrapping_sub(1);
                self.set_flag(cpu::FLAG_Z, sum == 0);
                self.set_flag(cpu::FLAG_N, true);
                self.set_flag(cpu::FLAG_H, (sum & 0x0F) == 0x0F);

                mmu.bus_write(self.hl().get(), sum);
            }
            _ => unreachable!(),
        }
    }

    pub fn opcode_rlca(&mut self, _mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        debug_assert!(opcode == 0x7);

        let val = self.a().get();
        let result = self.rlc(val);

        self.a().set(result);
        self.set_flag(cpu::FLAG_Z, false);
    }

    pub fn opcode_rrca(&mut self, _mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        debug_assert!(opcode == 0x0F);

        let val = self.a().get();
        let result = self.rrc(val);

        self.a().set(result);
        self.set_flag(cpu::FLAG_Z, false);
    }

    pub fn opcode_rla(&mut self, _mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        debug_assert!(opcode == 0x17);

        let val = self.a().get();
        let result = self.rl(val);

        self.a().set(result);
        self.set_flag(cpu::FLAG_Z, false);
    }

    pub fn opcode_rra(&mut self, _mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        debug_assert!(opcode == 0x1F);

        let val = self.a().get();
        let result = self.rr(val);

        self.a().set(result);
        self.set_flag(cpu::FLAG_Z, false);
    }

    pub fn opcode_add(&mut self, mmu: &mut MMU, instr: &Instruction, opcode: u8) {
        match (instr.dst, instr.src) {
            (OperandKind::R8, OperandKind::R8 | OperandKind::R16Addr) => {
                // ADD A, r8
                // ADD A, [HL]
                let src_reg_or_hladdr = opcode & 0x7;
                let src_val = self.read_r8(mmu, src_reg_or_hladdr);

                self.add_a8(src_val, 0);
            }
            (OperandKind::R16, OperandKind::R16) => {
                // ADD HL r16
                let src_reg = (opcode >> 4) & 0x3;
                let src_val = self.read_r16(src_reg);
                let dst_val = self.hl().get();

                let sum: u32 = u32::from(src_val) + u32::from(dst_val);

                self.set_flag(cpu::FLAG_N, false);
                self.set_flag(cpu::FLAG_H, (dst_val & 0xFFF) + (src_val & 0xFFF) > 0xFFF);
                self.set_flag(cpu::FLAG_C, sum > 0xFFFF);

                self.hl().set((sum & 0xFFFF) as u16);
            }
            (OperandKind::R8, OperandKind::Imm8) => {
                debug_assert!(opcode == 0xC6);
                let src_val = self.consume_byte_from_pc(mmu);

                self.add_a8(src_val, 0);
            }
            (OperandKind::R16, OperandKind::Imm8) => {
                // ADD SP e8
                debug_assert!(opcode == 0xE8);

                let sp_val = self.sp().get();
                let e: i8 = self.consume_byte_from_pc(mmu) as i8;
                let sum = i32::from(sp_val) + i32::from(e);

                self.set_flag(cpu::FLAG_Z, false);
                self.set_flag(cpu::FLAG_N, false);
                self.set_flag(cpu::FLAG_H, (i32::from(sp_val) ^ i32::from(e) ^ (i32::from(sum) & 0xFFFF)) & 0x10 == 0x10);
                self.set_flag(cpu::FLAG_C, (i32::from(sp_val) ^ i32::from(e) ^ (i32::from(sum) & 0xFFFF)) & 0x100 == 0x100);

                self.sp().set(sum as u16);
            }
            _ => unreachable!(),
        }
    }

    pub fn opcode_stop(&mut self, _mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        debug_assert!(opcode == 0x10);

        // println!("{}", self);
        // todo!("0x10");

        // Note: Enter CPU very low power mode. Also used to switch between double and normal speed CPU modes in GBC.
    }

    pub fn opcode_jr(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let branch_taken = match opcode {
            0x18 /* JR e8 */ => {
                true
            }
            _ => {
                debug_assert!([0x20, 0x28, 0x30, 0x38].contains(&opcode));
                let flag = (opcode >> 3) & 0x3;

                let cond = match flag {
                    0x3 => self.get_flag(cpu::FLAG_C),
                    0x2 => !self.get_flag(cpu::FLAG_C),
                    0x1 => self.get_flag(cpu::FLAG_Z),
                    0x0 => !self.get_flag(cpu::FLAG_Z),
                    _ => unreachable!(),
                };

                cond
            }
        };

        if branch_taken {
            let e: i8 = self.consume_byte_from_pc(mmu) as i8;
            let pc_val = self.pc().get();
            self.pc().set(pc_val.wrapping_add_signed(e.into()));
        } else {
            self.branch_skipped = true;
            self.pc().inc();
        }
    }

    pub fn opcode_daa(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) {
        let original_val = self.a().get();
        let flag_h = self.get_flag(cpu::FLAG_H);
        let flag_c = self.get_flag(cpu::FLAG_C);
        let flag_n = self.get_flag(cpu::FLAG_N);

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

        self.a().set(daa_value);
        self.set_flag(cpu::FLAG_C, carry_next);
        self.set_flag(cpu::FLAG_H, false);
        self.set_flag(cpu::FLAG_Z, daa_value == 0);
    }

    pub fn opcode_cpl(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) {
        let val = !self.a().get();
        self.a().set(val);
        self.set_flag(cpu::FLAG_N, true);
        self.set_flag(cpu::FLAG_H, true);
    }

    pub fn opcode_scf(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) {
        self.set_flag(cpu::FLAG_C, true);
        self.set_flag(cpu::FLAG_N, false);
        self.set_flag(cpu::FLAG_H, false);
    }

    pub fn opcode_ccf(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) {
        self.set_flag(cpu::FLAG_C, !self.get_flag(cpu::FLAG_C));
        self.set_flag(cpu::FLAG_N, false);
        self.set_flag(cpu::FLAG_H, false);
    }

    pub fn opcode_halt(&mut self, mmu: &mut MMU, _instr: &Instruction, _opcode: u8) {
        if self.ime {
            self.halted = true;
            return;
        }

        if mmu.bus_read(cpu::HREG_IE) & mmu.bus_read(cpu::HREG_IF) == 0 {
            self.halted = true;
            return;
        }

        // @todo Halt bug
        // The CPU continues execution after the HALT, but the byte after it is read twice in a row (PC is not incremented, due to a hardware bug). 
    }

    pub fn opcode_adc(&mut self, mmu: &mut MMU, instr: &Instruction, opcode: u8) {
        let src_val = self.consume_src_r8_imm8_hladdr(mmu, instr.src, opcode);
        self.add_a8(src_val, self.get_flag(cpu::FLAG_C).into());
    }

    pub fn opcode_sub(&mut self, mmu: &mut MMU, instr: &Instruction, opcode: u8) {
        let src_val = self.consume_src_r8_imm8_hladdr(mmu, instr.src, opcode);
        self.sub_a8(src_val, 0);
    }

    pub fn opcode_sbc(&mut self, mmu: &mut MMU, instr: &Instruction, opcode: u8) {
        let src_val = self.consume_src_r8_imm8_hladdr(mmu, instr.src, opcode);
        self.sub_a8(src_val, self.get_flag(cpu::FLAG_C).into());
    }

    pub fn opcode_and(&mut self, mmu: &mut MMU, instr: &Instruction, opcode: u8) {
        let src_val = self.consume_src_r8_imm8_hladdr(mmu, instr.src, opcode);
        self.and_a8(src_val);
    }

    pub fn opcode_xor(&mut self, mmu: &mut MMU, instr: &Instruction, opcode: u8) {
        let src_val = self.consume_src_r8_imm8_hladdr(mmu, instr.src, opcode);
        self.xor_a8(src_val);
    }

    pub fn opcode_or(&mut self, mmu: &mut MMU, instr: &Instruction, opcode: u8) {
        let src_val = self.consume_src_r8_imm8_hladdr(mmu, instr.src, opcode);
        self.or_a8(src_val);
    }

    pub fn opcode_cp(&mut self, mmu: &mut MMU, instr: &Instruction, opcode: u8) {
        let src_val = self.consume_src_r8_imm8_hladdr(mmu, instr.src, opcode);
        self.cp_a8(src_val);
    }

    pub fn opcode_ret(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let ret_taken = match opcode {
            0xC9 /* RET */ => {
                true
            }
            _ => {
                debug_assert!([0xC0, 0xC8, 0xD0, 0xD8].contains(&opcode));
                let flag = (opcode >> 3) & 0x3;

                let cond = match flag {
                    0x3 => self.get_flag(cpu::FLAG_C),
                    0x2 => !self.get_flag(cpu::FLAG_C),
                    0x1 => self.get_flag(cpu::FLAG_Z),
                    0x0 => !self.get_flag(cpu::FLAG_Z),
                    _ => unreachable!(),
                };

                cond
            }
        };

        if ret_taken {
            let ret_val = self.pop_u16(mmu);
            self.pc().set(ret_val);
        } else {
            self.branch_skipped = true;
        }
    }

    pub fn opcode_push(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = (opcode >> 4) & 0x3;
        let curr_val = self.read_r16stk(reg);
        self.push_u16(mmu, curr_val);
    }

    pub fn opcode_pop(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = (opcode >> 4) & 0x3;
        let val = self.pop_u16(mmu);
        self.write_r16stk(reg, val);
    }

    pub fn opcode_jp(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        if opcode == 0xE9 /* JP HL */ {
            let hl_val = self.hl().get();
            self.pc().set(hl_val);
            return;
        }

        let branch_taken = match opcode {
            0xC3 /* JP a16 */ => {
                true
            }
            _ => {
                let flag = (opcode >> 3) & 0x3;

                let cond = match flag {
                    0x3 => self.get_flag(cpu::FLAG_C),
                    0x2 => !self.get_flag(cpu::FLAG_C),
                    0x1 => self.get_flag(cpu::FLAG_Z),
                    0x0 => !self.get_flag(cpu::FLAG_Z),
                    _ => unreachable!(),
                };

                cond
            }
        };

        if branch_taken {
            let lsb = self.consume_byte_from_pc(mmu);
            let msb = self.consume_byte_from_pc(mmu);
            self.pc().set(util::value(msb, lsb));
        } else {
            self.branch_skipped = true;
            self.pc().inc();
            self.pc().inc();
        }
    }

    pub fn opcode_call(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let branch_taken = match opcode {
            0xCD /* CALL a16 */ => {
                true
            }
            _ => {
                debug_assert!([0xC4, 0xCC, 0xD4, 0xDC].contains(&opcode));
                let flag = (opcode >> 3) & 0x3;

                let cond = match flag {
                    0x3 => self.get_flag(cpu::FLAG_C),
                    0x2 => !self.get_flag(cpu::FLAG_C),
                    0x1 => self.get_flag(cpu::FLAG_Z),
                    0x0 => !self.get_flag(cpu::FLAG_Z),
                    _ => unreachable!(),
                };

                cond
            }
        };

        if branch_taken {
            let lsb = self.consume_byte_from_pc(mmu);
            let msb = self.consume_byte_from_pc(mmu);

            let pc_val = self.pc().get();
            self.push_u16(mmu, pc_val);
            self.pc().set(util::value(msb, lsb));
        } else {
            self.branch_skipped = true;
            self.pc().inc();
            self.pc().inc();
        }
    }

    pub fn opcode_rst(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let pc_val = self.pc().get();
        self.push_u16(mmu, pc_val);
        self.pc().set(util::value(0x0, opcode - 0xC7));
    }

    pub fn opcode_reti(&mut self, mmu: &mut MMU, _instr: &Instruction, _opcode: u8) {
        // RET
        let ret_val = self.pop_u16(mmu);
        self.pc().set(ret_val);
        self.ime = true;
    }

    pub fn opcode_di(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) {
        self.ime = false;
    }

    pub fn opcode_ei(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) {
        self.ime = true;
    }

    pub fn opcode_ldh(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let addr_u8 = self.consume_byte_from_pc(mmu);

        let addr_full = u16::from(addr_u8) | 0xFF00;

        match opcode {
            0xE0 /* LDH [a8] A */ => {
                mmu.bus_write(addr_full, self.a().get());
            }
            0xF0 /* LDH A [a8] */ => {
                let val = mmu.bus_read(addr_full);
                self.a().set(val);
            }
            _ => unreachable!(),
        }
    }

    pub fn opcode_prefix(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { unreachable!("0xCB prefix"); }
    pub fn opcode_illegal_d3(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { todo!("0xD3"); }
    pub fn opcode_illegal_db(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { todo!("0xDB"); }
    pub fn opcode_illegal_dd(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { todo!("0xDD"); }
    pub fn opcode_illegal_e3(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { todo!("0xE3"); }
    pub fn opcode_illegal_e4(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { todo!("0xE4"); }
    pub fn opcode_illegal_eb(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { todo!("0xEB"); }
    pub fn opcode_illegal_ec(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { todo!("0xEC"); }
    pub fn opcode_illegal_ed(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { todo!("0xED"); }
    pub fn opcode_illegal_f4(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { todo!("0xF4"); }
    pub fn opcode_illegal_fc(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { todo!("0xFC"); }
    pub fn opcode_illegal_fd(&mut self, _mmu: &mut MMU, _instr: &Instruction, _opcode: u8) { todo!("0xFD"); }

    // 0xCB instructions

    pub fn opcode_rlc(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = opcode & 0x7;
        let val = self.read_r8(mmu, reg);
        let new_val = self.rlc(val);
        self.write_r8(mmu, reg, new_val);
    }

    pub fn opcode_rrc(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = opcode & 0x7;
        let val = self.read_r8(mmu, reg);
        let new_val = self.rrc(val);
        self.write_r8(mmu, reg, new_val);
    }

    pub fn opcode_rl(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = opcode & 0x7;
        let val = self.read_r8(mmu, reg);
        let new_val = self.rl(val);
        self.write_r8(mmu, reg, new_val);
    }

    pub fn opcode_rr(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = opcode & 0x7;
        let val = self.read_r8(mmu, reg);
        let new_val = self.rr(val);
        self.write_r8(mmu, reg, new_val);
    }

    pub fn opcode_sla(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = opcode & 0x7;
        let val = self.read_r8(mmu, reg);

        // Strip least significant bit
        let new_val = self.rlc(val) & 0xFE;
        self.write_r8(mmu, reg, new_val);
    }

    pub fn opcode_sra(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = opcode & 0x7;
        let val = self.read_r8(mmu, reg);

        let high_bit = val & 0x80;

        let new_val = (self.rrc(val) & 0x7F) | high_bit;
        self.write_r8(mmu, reg, new_val);
    }

    pub fn opcode_swap(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = opcode & 0x7;
        let val = self.read_r8(mmu, reg);

        let low_4 = val & 0xF;
        let new_val = (val >> 4) | (low_4 << 4);

        self.set_flag(cpu::FLAG_Z, new_val == 0);
        self.set_flag(cpu::FLAG_N, false);
        self.set_flag(cpu::FLAG_H, false);
        self.set_flag(cpu::FLAG_C, false);

        self.write_r8(mmu, reg, new_val);
    }

    pub fn opcode_srl(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = opcode & 0x7;
        let val = self.read_r8(mmu, reg);

        let new_val = self.rrc(val) & 0x7F;
        self.write_r8(mmu, reg, new_val);
    }

    pub fn opcode_bit(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = opcode & 0x7;
        let bit_index = (opcode >> 3) & 0x7;

        let val = self.read_r8(mmu, reg);
        let bit_set = (val >> bit_index) & 0x1;
        
        self.set_flag(cpu::FLAG_Z, bit_set == 0);
        self.set_flag(cpu::FLAG_N, false);
        self.set_flag(cpu::FLAG_H, true);
    }

    pub fn opcode_res(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = opcode & 0x7;
        let bit_index = (opcode >> 3) & 0x7;

        let val = self.read_r8(mmu, reg);
        let new_val = val & !(0x1 << bit_index);

        self.write_r8(mmu, reg, new_val);
    }

    pub fn opcode_set(&mut self, mmu: &mut MMU, _instr: &Instruction, opcode: u8) {
        let reg = opcode & 0x7;
        let bit_index = (opcode >> 3) & 0x7;

        let val = self.read_r8(mmu, reg);
        let new_val = val | (0x1 << bit_index);

        self.write_r8(mmu, reg, new_val);
    }

    
    fn rrc(&mut self, value: u8) -> u8 {
        let carry_bit = value & 0x1;

        let val_full = (u16::from(value) >> 1) | (u16::from(carry_bit) << 7);

        let result = util::get_low(val_full);

        self.set_flag(cpu::FLAG_Z, result == 0);
        self.set_flag(cpu::FLAG_N, false);
        self.set_flag(cpu::FLAG_H, false);
        self.set_flag(cpu::FLAG_C, carry_bit == 0x1);

        return result;
    }

    fn rr(&mut self, value: u8) -> u8 {
        let carry_flag: bool = self.get_flag(cpu::FLAG_C);
        let carry_next = value & 0x1;

        let val_full = (u16::from(carry_flag) << 7) | (u16::from(value) >> 1);

        let result = util::get_low(val_full);

        self.set_flag(cpu::FLAG_Z, result == 0);
        self.set_flag(cpu::FLAG_N, false);
        self.set_flag(cpu::FLAG_H, false);
        self.set_flag(cpu::FLAG_C, carry_next == 0x1);

        return result;
    }

    fn rlc(&mut self, value: u8) -> u8 {
        let carry_bit = value & 0x80;

        let val_full = (u16::from(value) << 1) | (u16::from(carry_bit) >> 7);

        let result = util::get_low(val_full);

        self.set_flag(cpu::FLAG_Z, result == 0);
        self.set_flag(cpu::FLAG_N, false);
        self.set_flag(cpu::FLAG_H, false);
        self.set_flag(cpu::FLAG_C, carry_bit == 0x80);

        return result;
    }

    fn rl(&mut self, value: u8) -> u8 {
        let carry_flag: bool = self.get_flag(cpu::FLAG_C);
        let carry_next = value & 0x80;

        let val_full = (u16::from(value) << 1) | u16::from(carry_flag);

        let result = util::get_low(val_full);

        self.set_flag(cpu::FLAG_Z, result == 0);
        self.set_flag(cpu::FLAG_N, false);
        self.set_flag(cpu::FLAG_H, false);
        self.set_flag(cpu::FLAG_C, carry_next == 0x80);

        return result;
    }

    fn consume_src_r8_imm8_hladdr(&mut self, mmu: &mut MMU, src: OperandKind, opcode: u8) -> u8 {
        let val = match src {
            OperandKind::R8 => {
                let src_reg = opcode & 0x7;
                debug_assert!(src_reg != 0x6);
                debug_assert!(src_reg < 0x8);
                self.read_r8(mmu, src_reg)
            }
            OperandKind::R16Addr => {
                mmu.bus_read(self.hl().get())
            }
            OperandKind::Imm8 => {
                let val = self.consume_byte_from_pc(mmu);
                val
            }
            _ => unreachable!(),
        };
        val
    }

    fn add_a8(&mut self, val: u8, carry: u8) {
        let dst_val = self.a().get();

        let sum_full = u16::from(dst_val) + u16::from(val) + u16::from(carry);
        let sum_low = util::get_low(sum_full);
        
        self.set_flag(cpu::FLAG_Z, sum_low == 0);
        self.set_flag(cpu::FLAG_N, false);
        self.set_flag(cpu::FLAG_H, ((dst_val & 0xF) + (val & 0xF) + carry) > 0xF);
        self.set_flag(cpu::FLAG_C, sum_full > 0xFF);

        self.a().set(sum_low);
    }

    fn sub_a8(&mut self, val: u8, carry: u8) {
        let dst_val = self.a().get();

        let res_full = i16::from(dst_val) - i16::from(val) - i16::from(carry);
        let res_low = util::get_low(res_full as u16);

        let half_carry = ((dst_val as i8) & 0xF) - ((val as i8) & 0xF) - (carry as i8);
        
        self.set_flag(cpu::FLAG_Z, res_low == 0);
        self.set_flag(cpu::FLAG_N, true);
        self.set_flag(cpu::FLAG_H, half_carry < 0);
        self.set_flag(cpu::FLAG_C, res_full < 0);

        self.a().set(res_low);
    }

    fn and_a8(&mut self, val: u8) {
        let result = self.a().get() & val;

        self.set_flag(cpu::FLAG_Z, result == 0);
        self.set_flag(cpu::FLAG_N, false);
        self.set_flag(cpu::FLAG_H, true);
        self.set_flag(cpu::FLAG_C, false);

        self.a().set(result);
    }

    fn xor_a8(&mut self, val: u8) {
        let result = self.a().get() ^ val;

        self.set_flag(cpu::FLAG_Z, result == 0);
        self.set_flag(cpu::FLAG_N, false);
        self.set_flag(cpu::FLAG_H, false);
        self.set_flag(cpu::FLAG_C, false);

        self.a().set(result);
    }

    fn or_a8(&mut self, val: u8) {
        let result = self.a().get() | val;

        self.set_flag(cpu::FLAG_Z, result == 0);
        self.set_flag(cpu::FLAG_N, false);
        self.set_flag(cpu::FLAG_H, false);
        self.set_flag(cpu::FLAG_C, false);

        self.a().set(result);
    }

    fn cp_a8(&mut self, val: u8) {
        let a_val = self.a().get();
        self.sub_a8(val, 0);
        self.a().set(a_val);
    }
}
