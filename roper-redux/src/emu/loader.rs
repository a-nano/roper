extern crate unicorn;
extern crate goblin;

use std::fmt::{Debug,Formatter,Display};
use std::fmt;
use std::path::Path;
use std::sync::{Mutex,Arc};
use self::goblin::{Object,elf};
use self::unicorn::*;
use par::statics::*;

/* iT WOULD be nice to have a static map of the Elf/Object headers, for easy
 * reference, and to avoid the need to pass back copies of read-only memory,
 * for pointer dereference in mutation and fitness evaluation. 
 * Perhaps put this in par::statics. That would mean moving the goblin parsing
 * over there, which would also speed up the emu generation process. 
 */
const VERBOSE : bool = false;
pub const ARM_ARM   : Arch = Arch::Arm(Mode::Arm);
pub const ARM_THUMB : Arch = Arch::Arm(Mode::Thumb);
pub const STACK_SIZE: usize = 0x1000;

pub const PROT_READ: Perm = unicorn::PROT_READ;
pub const PROT_EXEC: Perm = unicorn::PROT_EXEC;
pub const PROT_WRITE: Perm = unicorn::PROT_WRITE;

pub type Perm = unicorn::Protection;
pub type MemImage = Vec<Seg>;

pub static MIPS_REGISTERS : [RegisterMIPS; 33] = [ RegisterMIPS::PC,
                                                   RegisterMIPS::ZERO,
                                                   RegisterMIPS::AT,
                                                   RegisterMIPS::V0,
                                                   RegisterMIPS::V1,
                                                   RegisterMIPS::A0,
                                                   RegisterMIPS::A1,
                                                   RegisterMIPS::A2,
                                                   RegisterMIPS::A3,
                                                   RegisterMIPS::T0,
                                                   RegisterMIPS::T1,
                                                   RegisterMIPS::T2,
                                                   RegisterMIPS::T3,
                                                   RegisterMIPS::T4,
                                                   RegisterMIPS::T5,
                                                   RegisterMIPS::T6,
                                                   RegisterMIPS::T7,
                                                   RegisterMIPS::S0,
                                                   RegisterMIPS::S1,
                                                   RegisterMIPS::S2,
                                                   RegisterMIPS::S3,
                                                   RegisterMIPS::S4,
                                                   RegisterMIPS::S5,
                                                   RegisterMIPS::S6,
                                                   RegisterMIPS::S7,
                                                   RegisterMIPS::T8,
                                                   RegisterMIPS::T9,
                                                   RegisterMIPS::K0,
                                                   RegisterMIPS::K1,
                                                   RegisterMIPS::GP,
                                                   RegisterMIPS::SP,
                                                   RegisterMIPS::FP,
                                                   RegisterMIPS::RA ];

pub static ARM_REGISTERS : [RegisterARM; 16] = [RegisterARM::R0,
                                                RegisterARM::R1,
                                                RegisterARM::R2,
                                                RegisterARM::R3,
                                                RegisterARM::R4,
                                                RegisterARM::R5,
                                                RegisterARM::R6,
                                                RegisterARM::R7,
                           /****************/   RegisterARM::R8,
                           /****************/   RegisterARM::SB,
                           /* Not used in  */   RegisterARM::SL,
                           /* Thumb Mode   */   RegisterARM::FP,
                           /****************/   RegisterARM::IP,
                           /****************/   RegisterARM::SP,
                                                RegisterARM::LR,
                                                RegisterARM::PC];


/* TODO: implement clone for unicorn */
pub enum Emu {
    UcArm(unicorn::CpuARM),
    UcMips(unicorn::CpuMIPS),
}


impl Debug for Emu {
    fn fmt (&self, f: &mut Formatter) -> fmt::Result {
        match self {
            &Emu::UcArm(_) => write!(f, "Unicorn ARM CPU with regions: {:?}", self.mem_regions()),
            &Emu::UcMips(_) => write!(f, "Unicorn MIPS CPU with regions: {:?}", self.mem_regions()), 
        }
    }
}

impl Emu {
    pub fn mem_write(&mut self, addr: u64, data: &Vec<u8>) -> Result<(), unicorn::Error> {
        match self {
            &mut Emu::UcArm(ref mut uc) => uc.mem_write(addr, data),
            &mut Emu::UcMips(ref mut uc) => uc.mem_write(addr, data),
        }
    }

    pub fn mem_map(&mut self, address: u64, size: usize, perms: Protection)
        -> Result<(), Error> 
    {
        match self {
            &mut Emu::UcArm(ref mut uc) => uc.mem_map(address, size, perms),
            &mut Emu::UcMips(ref mut uc) => uc.mem_map(address, size, perms),
        }
    }

    pub fn mem_read(&self, address: u64, size: usize) -> Result<Vec<u8>, Error> {
        match self {
            &Emu::UcArm(ref uc) => uc.mem_read(address, size),
            &Emu::UcMips(ref uc) => uc.mem_read(address, size),
        }
    }

    pub fn mem_regions(&self) -> Result<Vec<MemRegion>, unicorn::Error> {
        match self {
            &Emu::UcArm(ref uc)  => uc.mem_regions(),
            &Emu::UcMips(ref uc) => uc.mem_regions(),
        }
    }

    pub fn query(&self, query: Query) -> Result<usize, Error> {
        match self {
            &Emu::UcArm(ref uc) => uc.query(query),
            &Emu::UcMips(ref uc) => uc.query(query),
        }
    }

    pub fn get_mode(&self) -> Mode {
        match self.query(Query::MODE) {
            Ok(n) => {
                let m = umode_from_usize(n);
                match m {
                    unicorn::Mode::LITTLE_ENDIAN => match self {
                        &Emu::UcArm(_) => Mode::Arm,
                        _ => Mode::Le,
                    },
                    unicorn::Mode::THUMB => Mode::Thumb,
                    _ => panic!("unimplemented"),

                }
            },
            _ => panic!("Failed to get mode"),
        }
    }

    pub fn add_code_hook<F>(&mut self,
                         hooktype: unicorn::CodeHookType,
                         start_addr: u64,
                         stop_addr: u64,
                         callback: F)
        -> Result<unicorn::uc_hook, Error>
        where F: Fn(&Unicorn, u64, u32) -> () + 'static,
    {
        match self {
            &mut Emu::UcArm(ref mut uc) => uc.add_code_hook(hooktype,
                                                         start_addr,
                                                         stop_addr,
                                                         callback),
            &mut Emu::UcMips(ref mut uc) => uc.add_code_hook(hooktype,
                                                          start_addr,
                                                          stop_addr,
                                                          callback),
        }
    }

    pub fn hook_writeable_mem<F> (&mut self,
                                  callback: F)
        -> Result<unicorn::uc_hook, Error>
        where 
            F: Fn(&Unicorn, 
                  unicorn::MemType, 
                  u64, 
                  usize, 
                  i64) -> bool + 'static 
    {
        
        let writeable = self.writeable_memory();
        let mut begin = None;
        let mut end = None;
        for seg in &writeable {
            let b = seg.aligned_start();
            let e = seg.aligned_end();
            if begin == None || b < begin.unwrap() { begin = Some(b) };
            if end == None || e > end.unwrap() { end = Some(e) };
        }
        assert!(begin != None && end != None);
        self.add_mem_hook(unicorn::MemHookType::MEM_WRITE,
                          begin.unwrap(),
                          end.unwrap(),
                          callback)
    }

    pub fn add_mem_hook<F> (&mut self,
                            hook_type: unicorn::MemHookType,
                            begin: u64,
                            end: u64,
                            callback: F) -> Result<uc_hook, Error>
        where F: Fn(&Unicorn, MemType, u64, usize, i64) -> bool + 'static,
    {
        match self {
            &mut Emu::UcArm(ref mut uc) => uc.add_mem_hook(hook_type,
                                                           begin,
                                                           end,
                                                           callback),
            &mut Emu::UcMips(ref mut uc) => uc.add_mem_hook(hook_type,
                                                            begin,
                                                            end,
                                                            callback),
        }
    }
            
                            

    pub fn hook_exec_mem<F> (&mut self,
                             callback: F) 
        -> Result<unicorn::uc_hook, Error>
        where F: Fn(&Unicorn, u64, u32) -> () + 'static, 
    {
        let regions = self.mem_regions().unwrap();
        let mut exec_start = None;
        let mut exec_stop = None;
        for region in regions {
            if !region.perms.intersects(PROT_EXEC) { continue; }
            if exec_start == None || region.begin < exec_start.unwrap() {
                exec_start = Some(region.begin)
            };
            if exec_stop == None || region.end > exec_stop.unwrap() {
                exec_stop = Some(region.end)
            };
        }
        if exec_start == None || exec_stop == None {
            Err(unicorn::Error::ARG)
        } else {
            //println!("> exec_start: {:08x}, exec_stop: {:08x}", exec_start.unwrap(), exec_stop.unwrap());
            self.add_code_hook(unicorn::CodeHookType::CODE,
                               exec_start.unwrap(),
                               exec_stop.unwrap(),
                               callback)
        }
    }

    pub fn hook_interrupts<F> (&mut self,
                               callback: F)
        -> Result<unicorn::uc_hook, Error>
        where F: Fn(&Unicorn, u32) -> () + 'static,
    {
        match self {
            &mut Emu::UcArm(ref mut uc) => uc.add_intr_hook(callback),
            &mut Emu::UcMips(ref mut uc) => uc.add_intr_hook(callback),
        }

    }

    /*
    pub fn add_insn_in_hook<F>(&mut self, callback: F) -> Result<uc_hook, Error>
        where F: Fn(&Unicorn, u32, usize) -> u32 + 'static,
    {
        /* TODO: write a macro that does this repetitive "match self" business */
        match self {
            &mut Emu::UcArm(ref mut uc) => uc.add_insn_in_hook(callback),
            &mut Emu::UcMips(ref mut uc) => uc.add_insn_in_hook(callback),
        }
    }
*/
    pub fn remove_hook(&mut self, uc_hook: unicorn::uc_hook) -> Result<(),Error> {
        match self {
            &mut Emu::UcArm(ref mut uc) => uc.remove_hook(uc_hook),
            &mut Emu::UcMips(ref mut uc) => uc.remove_hook(uc_hook),
        }
    }

    pub fn risc_width(&self) -> u64 {
        match self.get_mode() {
            Mode::Arm => 4,
            Mode::Thumb => 2,
            _ => panic!("unimplemented!"),
        }
    }

    /* I prefer having usize here, instead of i32 */
    pub fn read_general_registers(&self) -> Result<Vec<u64>, Error> {
        match self {
            &Emu::UcArm(ref uc) => {
                Ok(ARM_REGISTERS.iter()
                                .map(|&x| uc.reg_read(x)
                                            .expect("Error reading registers"))
                                .collect())
                                         
            }
            &Emu::UcMips(ref uc) => {
                Ok(MIPS_REGISTERS.iter()
                                 .map(|&x| uc.reg_read(x)
                                             .expect("Error reading registers"))
                                 .collect())
            }
        }
    }


    pub fn start(&mut self,
                 begin: u64, 
                 until: u64, 
                 timeout: u64, 
                 count: usize) 
        -> Result<(), Error> 
    {
        match self {
            &mut Emu::UcArm(ref mut uc)  => uc.emu_start(begin, until, timeout, count),
            &mut Emu::UcMips(ref mut uc) => uc.emu_start(begin, until, timeout, count),
        }
    }

    pub fn find_stack (&self) -> (u64, usize) {
        let regions = self.mem_regions().unwrap();
        let mut bottom : Option<u64> = None;
        let mut stack : Option<MemRegion> = None;
        for region in regions.iter() {
            if region.perms.intersects(PROT_READ|PROT_WRITE) &&
               region.begin >= bottom.unwrap_or(0) {
                   bottom = Some(region.begin);
                   stack = Some(region.clone());
               };
        };
        let stack = stack
                    .expect(
                       &format!("[!] Could not find stack bottom! Regions: {:?}",
                                regions));
        (stack.begin, (stack.end - stack.begin) as usize)
    }

    pub fn set_sp (&mut self, val: u64) -> Result<(),Error> {
        match self {
            &mut Emu::UcArm(ref mut uc) => {
                let sp = RegisterARM::SP;
                uc.reg_write(sp, val)
            },
            &mut Emu::UcMips(ref mut uc) => {
                let sp = RegisterMIPS::SP;
                uc.reg_write(sp, val)
            },
        }
    }

    pub fn writeable_memory (&self) -> MemImage {
        let mut wmem = Vec::new();
        for rgn in self.mem_regions()
                       .unwrap()
                       .iter()
                       .filter(|r| r.perms.intersects(PROT_WRITE)) 
        {
            let data: Vec<u8> = self.mem_read(rgn.begin,
                                              (rgn.end-rgn.begin) as usize)
                                    .unwrap();
            wmem.push( Seg {
                addr: rgn.begin,
                perm: rgn.perms,
                memsz: (rgn.end - rgn.begin) as usize,
                data: data,
                segtype: SegType::Load,
            });
        }
        wmem
    }
}


pub fn umode_from_usize(x: usize) -> unicorn::Mode {
    match x {
        0 => unicorn::Mode::LITTLE_ENDIAN,
        2 => unicorn::Mode::MODE_16,
        4 => unicorn::Mode::MODE_32,
        8 => unicorn::Mode::MODE_64,
        16 => unicorn::Mode::THUMB,
        32 => unicorn::Mode::MCLASS,
        64 => unicorn::Mode::V8,
        0x40000000 => unicorn::Mode::BIG_ENDIAN,
        _ => unicorn::Mode::LITTLE_ENDIAN,
    }
}
//unsafe impl Sync for Emu { }
//unsafe impl Send for Emu {}

impl Clone for Emu {
    fn clone(&self) -> Self {
        let regions = self.mem_regions().unwrap();
        let uc_mode = umode_from_usize(self.query(Query::MODE).unwrap());
        let mut new : Emu = match self {
            &Emu::UcArm(_) => Emu::UcArm(CpuARM::new(uc_mode).unwrap()),
            &Emu::UcMips(_) => Emu::UcMips(CpuMIPS::new(uc_mode).unwrap()),
        };
        /* map the same regions, and copy the data */
        for region in regions {
            let addr = region.begin;
            let size = (region.end - region.begin) as usize;
            let perms = region.perms;
            new.mem_map(addr, size, perms);
            /* read the data for each region */
            let data = self.mem_read(addr, size).unwrap();
            new.mem_write(addr, &data);
        }
        /* now, the registers */
        /* meh. later. TODO */
        new
    }
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Mode {
    Arm,
    Thumb,
    Be,
    Le,
}

impl Mode {
    pub fn as_uc(&self) -> unicorn::Mode {
        match self {
            &Mode::Arm => unicorn::Mode::LITTLE_ENDIAN,
            &Mode::Thumb => unicorn::Mode::THUMB,
            &Mode::Be => unicorn::Mode::BIG_ENDIAN,
            &Mode::Le => unicorn::Mode::LITTLE_ENDIAN,
        }
    }
}

#[derive(PartialEq,Eq,Debug)]
pub enum Arch {
    Arm(Mode),
    Mips(Mode),
}

impl Arch {
    pub fn as_uc(&self) -> (unicorn::Arch, unicorn::Mode) {
        match self {
            &Arch::Arm(ref m) => (unicorn::Arch::ARM, m.as_uc()),
            &Arch::Mips(ref m) => (unicorn::Arch::MIPS, m.as_uc()),
        }
    }
    pub fn mode(&self) -> Mode {
        match self {
            &Arch::Arm(ref m) => m.clone(),
            &Arch::Mips(ref m) => m.clone(),
        }
    }
    //pub fn as_cs(&self) -> capstone::
}

#[derive(Copy,Clone,PartialEq,Eq,Debug)]
pub enum SegType {
    Null,
    Load,
    Dynamic,
    Interp,
    Note,
    ShLib,
    PHdr,
    Tls,
    GnuEhFrame,
    GnuStack,
    GnuRelRo,
    Other, /* KLUDGE: a temporary catchall */
}

impl SegType {
    fn new(raw: u32) -> Self {
        match raw {
            0 => SegType::Null,
            1 => SegType::Load,
            2 => SegType::Dynamic,
            3 => SegType::Interp,
            4 => SegType::Note,
            5 => SegType::ShLib,
            6 => SegType::PHdr,
            7 => SegType::Tls,
            0x6474e550 => SegType::GnuEhFrame,
            0x6474e551 => SegType::GnuStack,
            0x6474e552 => SegType::GnuRelRo,
            _ => SegType::Other,
        }
    }
    pub fn loadable (&self) -> bool {
        match self {
            &SegType::Load => true,
            _ => false,
        }
    }
}

#[derive(PartialEq,Eq,Debug,Clone)]
pub struct Seg {
    pub addr: u64,
    pub memsz: usize,
    pub perm: Perm,
    pub segtype: SegType,
    pub data: Vec<u8>,
}

impl Display for Seg {
    fn fmt (&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "[aligned {:08x} -- {:08x}: {:?}]",
               self.aligned_start(),
               self.aligned_end(),
               self.perm)
    }
}

impl Seg {
    pub fn from_phdr(phdr: &elf::ProgramHeader) -> Self {
        let mut uc_perm = PROT_NONE;
        if phdr.is_executable() { uc_perm |= PROT_EXEC  };
        if phdr.is_write()      { uc_perm |= PROT_WRITE };
        if phdr.is_read()       { uc_perm |= PROT_READ  };
        let size = (phdr.vm_range().end - phdr.vm_range().start) as usize;
        let data = vec![0; size];
        Seg {
            addr:    phdr.vm_range().start as u64,
            memsz:   size,
            perm:    uc_perm,
            segtype: SegType::new(phdr.p_type),
            data:    data,
        }
    }
    pub fn aligned_start(&self) -> u64 {
        self.addr & 0xFFFFF000
    }
    pub fn aligned_end(&self) -> u64 {
        (self.addr + (self.memsz as u64) + 0x1000) & 0xFFFFF000
    }
    pub fn aligned_size(&self) -> usize {
        ((self.addr as usize & 0x0FFF) + self.memsz as usize + 0x1000) & 0xFFFFF000
    }
    pub fn loadable(&self) -> bool {
        self.segtype.loadable()
    }
}

/* from raw Unicorn instance. Useful inside callbacks, for disassembling */
pub fn get_uc_mode(uc: &Unicorn) -> Mode {
    /* TODO keep a global static architecture variable, for reference
     * in situations like these. for now, we're just assuming ARM, but
     * plan to extend the system to cover, at least, MIPS, too.
     */
    let raw = uc.query(unicorn::Query::MODE).unwrap();

    match raw {
        0b00000 => Mode::Arm,
        0b10000 => Mode::Thumb,
        _ => panic!("Mode not recognized"),
    }
}

pub fn init_emulator_with_code_buffer (archmode: &Arch) -> Result<Emu,unicorn::Error> {
    init_emulator(&CODE_BUFFER, archmode)
}


pub fn init_emulator (buffer: &Vec<u8>, archmode: &Arch) -> Result<Emu,unicorn::Error> { 

    let obj = Object::parse(&buffer).unwrap();
    let (arch, mode) = archmode.as_uc();

    /* stopgap */
    assert_eq!(arch, unicorn::Arch::ARM);
    let mut uc = CpuARM::new(mode).expect("Failed to create CpuARM");
    let mem_image: MemImage = MEM_IMAGE.to_vec();
    for seg in mem_image {
        /* segment is: (addr, perm, aligned_size, data) */ 
        uc.mem_map(seg.aligned_start(), seg.aligned_size(), seg.perm)?; //.expect(&format!("Mapping error for {:?}", seg));
        uc.mem_write(seg.addr, &seg.data)?;
    }
    //println!("regions: {:?}", uc.mem_regions()?);
    Ok(Emu::UcArm(uc))

    /*
    if let Object::Elf(e) = obj {
        let string_table = &e.shdr_strtab;
        let sname = |s: &elf::SectionHeader| string_table.get(s.sh_name);
        let phdrs = &e.program_headers;
        let shdrs = &e.section_headers;

        /* first, map the segments */
        for phdr in phdrs {
            /* get permissions */

            let seg = Seg::from_phdr(&phdr);
            if seg.loadable() {
                if VERBOSE && cfg!(debug_assertions) {
                    println!("[+] mapping {:?}",seg);
                }
                uc.mem_map(seg.aligned_start(), seg.aligned_size(), seg.perm)
                  .expect(&format!("Failed to map segment {:?}",seg))
            }
        }
        /* map a page of low memory, since elf likes to write some info there */
        uc.mem_map(0, 0x1000, PROT_READ)
          .expect("Failed to map 0x1000 bytes of low memory.");
        /* now allocate a stack */
        let mut bottom = 0;
        let regions = uc.mem_regions().unwrap();
        for region in &regions {
            if VERBOSE && cfg!(debug_assertions) {
                println!("[+] region: {:x} - {:x} {:?}", 
                     region.begin, region.end, region.perms);
            }
            if region.end > bottom { bottom = region.end + 1; };
        }
        if VERBOSE && cfg!(debug_assertions) {
            println!("[+] mapping stack: {:x} bytes at {:x}", STACK_SIZE, bottom);
        }
        uc.mem_map(bottom, STACK_SIZE, PROT_READ|PROT_WRITE)
          .expect(&format!("Failed to allocate stack of {:x} bytes at {:x}",
                           STACK_SIZE, bottom));
        /* now, write the sections */
        for shdr in shdrs {
            if VERBOSE && cfg!(debug_assertions) {
                println!("shdr {:?}> {:?}",sname(&shdr), shdr);
            }
            let (i,j) = (shdr.sh_offset as usize, (shdr.sh_offset+shdr.sh_size) as usize);
            let aj = usize::min(j, buffer.len());
            let sdata = buffer[i..aj].to_vec();
            uc.mem_write(shdr.sh_addr, &sdata)
              .expect(&format!("Failed to write {:x} bytes of data to uc addr {:x}",
                               sdata.len(), shdr.sh_addr)); 

        }
        /* some debugging prints to see if this worked */

        Ok(Emu::UcArm(uc))
    } else {
        Err(format!("Lucca didn't finish this function"))
    }
*/

}


lazy_static! {
    pub static ref MEM_IMAGE: MemImage
        = {
            let obj = Object::parse(&CODE_BUFFER).unwrap();
            let mut segs: Vec<Seg> = Vec::new();
            match obj {
                Object::Elf(e) => {
                    let shdrs = &e.section_headers;

                    let phdrs = &e.program_headers;
                    for phdr in phdrs {
                        let seg = Seg::from_phdr(&phdr);
                        if seg.loadable() {
                            let start = seg.aligned_start() as usize;
                            let end = seg.aligned_end() as usize;
                            segs.push(seg);
                        }
                    }
                    /* Low memory */
                    segs.push(Seg { addr: 0,
                                    memsz: 0x1000,
                                    perm: PROT_READ,
                                    segtype: SegType::Load,
                                    data: vec![0; 0x1000],
                    });

                    for shdr in shdrs {
                        let (i,j) = (shdr.sh_offset as usize, 
                                     (shdr.sh_offset+shdr.sh_size) as usize);
                        let aj = usize::min(j, CODE_BUFFER.len());
                        let sdata = CODE_BUFFER[i..aj].to_vec();
                        /* find the appropriate segment */
                        let mut s = 0;
                        
                        for seg in segs.iter_mut() {
                            if shdr.sh_addr >= seg.aligned_start()
                                && shdr.sh_addr < seg.aligned_end() {
                                /* then we found a fit */
                                /* copy over the section data, at the correct offset */
                                let mut v_off 
                                    = (shdr.sh_addr - seg.aligned_start()) as usize;
                                for byte in sdata {
                                    seg.data[v_off] = byte;
                                    v_off += 1;
                                }
                                break;
                            }
                            s += 1;
                        }
                    }
                    /* now allocate the stack */
                    let mut bottom = 0;
                    for seg in &segs {
                        let b = seg.aligned_end();
                        if b > bottom { bottom = b };
                    }
                    segs.push(Seg { addr: bottom,
                                    perm: PROT_READ|PROT_WRITE,
                                    segtype: SegType::Load,
                                    memsz: STACK_SIZE,
                                    data: vec![0; STACK_SIZE]
                    });

                },
                _ => panic!("Not yet implemented."),
            }
            segs
        };
}

fn find_static_seg (addr: u64) -> Option<&'static Seg> {
    let mut this_seg = None;
    for seg in MEM_IMAGE.iter() {
        if seg.aligned_start() <= addr && addr < seg.aligned_end() {
            this_seg = Some(seg);
        };
    }
    this_seg
}

pub fn read_static_mem (addr: u64, size: usize) -> Option<Vec<u8>> {
    if let Some(seg) = find_static_seg(addr) {
        let offset = (addr - seg.aligned_start()) as usize;
        let offend = offset + size;
        Some(seg.data[offset..offend].to_vec())
    } else { None }
}

#[test]
fn test_init_emulator_with_MEM_IMAGE() {
    init_emulator_with_code_buffer(&ARM_ARM).unwrap();
}
