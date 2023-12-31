use std::fs::File;
use std::io::BufReader;

use anyhow::{Error, Result};
use structopt::StructOpt;

use crate::cli::Cli;
use crate::constant::ADDRESS_BRK;
use crate::constant::ADDRESS_TEST_PROGRAM;
use crate::constant::MEMORY_MAX;
use crate::constant::NEGATIVE_FLAG;
use crate::constant::PC_ADDRESS_RESET;
use crate::constant::PRG_ROM_ADDRESS;
use crate::cpu::debugger::CpuDebugger;
use crate::cpu::instruction::CpuInstruction;
use crate::cpu::opcode::{Operation, OPCODE_TABLE};
use crate::mem::Mem;
use crate::stack::get_sp_offset;
use crate::stack::Stacked;

use super::CpuRegister;

// reference: https://www.nesdev.org/wiki/CPU_registers

#[derive(Debug)]
pub struct Cpu6502 {
    pub debugger: CpuDebugger<u8>,
    pub clocks_to_pause: u8,
    pub registers: CpuRegister,
    /// NES memory uses 16-bit for memory addressing
    /// The stack address space is hardwired to memory page $01, i.e. the address range $0100–$01FF (256–511)
    pub mapper: [u8; MEMORY_MAX], // 64KB
    pub instr: Option<CpuInstruction>, // The currently executing instruction
}

impl Default for Cpu6502 {
    fn default() -> Self {
        let debugger = CpuDebugger::default();
        Self {
            debugger,
            clocks_to_pause: 0,
            registers: CpuRegister::default(),
            mapper: [0u8; MEMORY_MAX],
            instr: None,
        }
    }
}

pub trait Clocked {
    fn clocked(self: &mut Self) -> Result<bool>;
}

impl Clocked for Cpu6502 {
    fn clocked(self: &mut Self) -> Result<bool> {
        // // load cpu program counter register at $8000
        if let Ok(opcode) = self.mem_read(self.registers.pc) {
            let (addr, addr_value, num_bytes, mut instr) =
                self.decode_instruction(opcode as u8).unwrap();

            instr.mode_args = addr_value;
            instr.write_target = addr;

            if instr.opcode == Operation::BRK {
                self.debugger.debug_instr(self, instr);
                return Ok(false);
            }

            self.instr = Some(instr);

            // Debug the instruction
            self.debugger.debug_instr(self, instr);

            println!("Program counter {:0x?}", self.registers.pc);

            self.registers.pc = self.registers.pc.wrapping_add(num_bytes);
            self.execute_instruction(&instr)?;

            println!("After => Program counter {:0x?}", self.registers.pc);

            self.clocks_to_pause = self.clocks_to_pause.wrapping_add(instr.cycle - 1);
            return Ok(true);
        }
        return Ok(false);
    }
}

impl Stacked for Cpu6502 {
    #[must_use]
    #[inline]
    fn push_stack(&mut self, val: u8) -> Result<()> {
        let sp = self.registers.sp;
        // decrease the stack pointer by 1, write to the base + offset address
        self.mem_write(get_sp_offset(sp), val)?;
        self.registers.sp = self.registers.sp.wrapping_sub(1);
        Ok(())
    }

    #[must_use]
    #[inline]
    fn pop_stack(&mut self) -> Result<u8> {
        // increase the stack pointer by 1, read from the base + offset address
        self.registers.sp = self.registers.sp.wrapping_add(1);
        self.mem_read(get_sp_offset(self.registers.sp))
    }
}

impl Mem for Cpu6502 {
    fn mem_read(&self, addr: u16) -> Result<u8> {
        match addr {
            0x0000..=0x1fff => {
                // Mask to zero out the highest two bits in a 16-bit address
                let mirror_down_addr = addr & 0b00000111_11111111;
                println!("Read from address {:0x?}", mirror_down_addr);
                Ok(self.mapper[mirror_down_addr as usize])
            }
            _ => Ok(self.mapper[addr as usize]),
        }
    }

    fn mem_write(&mut self, addr: u16, data: u8) -> Result<()> {
        self.mapper[addr as usize] = data;
        Ok(())
    }
}

impl Cpu6502 {
    // memory
    pub fn read_write_target(&self, write_target: Option<u16>) -> Result<u8> {
        Ok(match write_target {
            None => self.registers.a,
            Some(ptr) => self.mem_read(ptr)?,
        })
    }

    pub fn store_write_target(&mut self, v: u8, write_target: Option<u16>) -> Result<()> {
        match write_target {
            None => self.registers.a = v,
            Some(ptr) => {
                self.mem_write(ptr, v)?;
            }
        }
        Ok(())
    }

    // instruction handler utilities
    pub fn is_negative(&self, result: u8) -> bool {
        (result & 0x80) == NEGATIVE_FLAG
    }

    pub fn update_zero_and_negative_flags(&mut self, result: u8) {
        self.registers.zero = result == 0x0;
        // bit masking and get the first bit
        self.registers.negative = self.is_negative(result);
    }

    pub fn update_accumulator_flags(&mut self) {
        self.update_zero_and_negative_flags(self.registers.a);
    }

    #[allow(dead_code)]
    pub fn print_register_status(&self) {
        println!("Carry: {:?}", self.registers.carry);
        println!("Decimal: {:?}", self.registers.decimal);
        println!("Negative: {:?}", self.registers.negative);
        println!("Overflow: {:?}", self.registers.overflow);
        println!(
            "interrupt_disabled: {:?}",
            self.registers.interrupt_disabled
        );
        println!("Zero: {:?}", self.registers.zero);
    }

    #[allow(unused)]
    pub fn set_status_register_from_byte(&mut self, v: u8) {
        // N.O._._.D.I.Z.C
        self.registers.carry = v & 0b00000001 > 0;
        self.registers.zero = v & 0b00000010 > 0;
        self.registers.interrupt_disabled = v & 0b00000100 > 0;
        self.registers.decimal = v & 0b00001000 > 0;
        // Break isn't a real register
        // Bit 5 is unused
        self.registers.overflow = v & 0b01000000 > 0;
        self.registers.negative = v & 0b10000000 > 0;
    }

    #[allow(unused)]
    pub fn status_register_byte(&self, is_instruction: bool) -> u8 {
        let result = ((self.registers.carry      as u8) << 0) |
            ((self.registers.zero       as u8) << 1) |
            ((self.registers.interrupt_disabled as u8) << 2) |
            ((self.registers.decimal    as u8) << 3) |
            (0                       << 4) | // Break flag
            ((if is_instruction {1} else {0}) << 5) |
            ((self.registers.overflow   as u8) << 6) |
            ((self.registers.negative   as u8) << 7);
        return result;
    }

    pub fn reset(&mut self) -> Result<()> {
        self.instr = None;

        self.registers.a = 0;
        self.registers.x = 0;
        // // Reset the address of program counter
        self.registers.pc = self.mem_read_u16(PC_ADDRESS_RESET).unwrap();
        Ok(())
    }

    #[allow(unused)]
    pub fn load_program(self: &mut Self, program: Vec<u8>) -> Result<()> {
        // $8000–$FFFF: ROM and mapper registers ((see MMC1 and UxROM for examples))
        let program_rom_address = PRG_ROM_ADDRESS as usize;
        self.mapper[program_rom_address..(program_rom_address + program.len())]
            .copy_from_slice(&program[..]);

        // Write the value of program counter as the start address of PRG ROM
        self.mem_write_u16(PC_ADDRESS_RESET, PRG_ROM_ADDRESS)
            .unwrap();

        assert_eq!(
            self.mem_read_u16(PC_ADDRESS_RESET).unwrap(),
            PRG_ROM_ADDRESS
        );

        // Reset the cpu after loading the program
        self.reset()?;

        Ok(())
    }

    pub fn load_test_program(self: &mut Self, program: Vec<u8>) -> Result<()> {
        let program_rom_address = PRG_ROM_ADDRESS as usize;
        self.mapper[program_rom_address..(program_rom_address + program.len())]
            .copy_from_slice(&program[..]);

        // Write the value of program counter as the start address of PRG ROM
        self.mem_write_u16(ADDRESS_TEST_PROGRAM, PRG_ROM_ADDRESS)
            .unwrap();

        assert_eq!(
            self.mem_read_u16(ADDRESS_TEST_PROGRAM).unwrap(),
            PRG_ROM_ADDRESS
        );

        Ok(())
    }

    pub fn run(self: &mut Self) -> Result<()> {
        let mut clock_status = true;
        while clock_status && self.registers.pc != ADDRESS_BRK {
            if self.clocks_to_pause > 0 {
                self.clocks_to_pause -= 1;
            }
            clock_status = self.clocked()?;
        }
        Ok(())
    }

    #[allow(unused)]
    pub fn bounded_run(self: &mut Self, steps: usize) -> Result<()> {
        for _ in 0..steps {
            let clock_status = self.clocked()?;
            if !clock_status {
                break;
            }
        }
        Ok(())
    }

    fn decode_instruction(
        self: &Self,
        opcode: u8,
    ) -> Result<(Option<u16>, u16, u16, CpuInstruction)> {
        let (opcode, address_mode, cycle, extra_cycle) = &OPCODE_TABLE[opcode as usize];
        let (addr, addr_value, num_bytes) = self.decode_addressing_mode(*address_mode)?;
        Ok((
            addr,
            addr_value,
            num_bytes + 1,
            CpuInstruction {
                opcode: *opcode,
                cycle: *cycle,
                address_mode: *address_mode,
                extra_cycle: *extra_cycle,
                write_target: None,
                mode_args: 0,
            },
        ))
    }

    fn execute_instruction(self: &mut Self, instruction: &CpuInstruction) -> Result<(), Error> {
        macro_rules! execute_opcode {
            ($($opcode:ident),*) => {
                match instruction.opcode {
                    $(
                        Operation::$opcode => self.$opcode(),
                    )*
                    _ => unimplemented!()
                }
            };
        }
        return execute_opcode!(
            ADC, AND, ASL, // Axx
            BCC, BCS, BEQ, BIT, BMI, BNE, BPL, BRK, BVC, BVS, // Bxx
            CLC, CLD, CLI, CLV, CMP, CPX, CPY, // Cxx
            DEC, DEX, DEY, // Dxx
            EOR, // Exx
            INC, INX, INY, // Ixx
            JMP, JSR, // Jxx
            LDA, LDX, LDY, LSR, // Lxx
            NOP, // Nxx
            ORA, // Oxx
            PHA, PHP, PLA, PLP, // Pxx
            ROL, ROR, RTI, RTS, // Rxx
            SBC, SEC, SED, SEI, STA, STX, STY, // Sxx
            TAX, TXA, TAY, TYA, TXS // Txx
        );
    }

    /// Read image from a provided input path
    #[allow(dead_code)]
    fn load_image(self: &mut Self) {
        let cli = Cli::from_args();

        let f = File::open(cli.path).expect("couldn't open file");
        let f = BufReader::new(f);
        println!("{}", f.capacity());
    }
}
