#![feature(asm)]
#![feature(global_asm)]
/*
 * This file is part of the coreboot project.
 *
 * Copyright 2018 Philipp Hug <philipp@hug.cx>
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; version 2 of the License.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 */

// 33.33 Mhz after reset
const FU540_BASE_FQY: usize = 33330;

use clock::ClockNode;
use core::{ops, ptr};
use model::*;

use crate::reg;
use crate::is_qemu;
use register::mmio::{ReadOnly, ReadWrite};
use register::{register_bitfields, Field};

#[allow(non_snake_case)]
#[repr(C)]

pub struct RegisterBlock {
    Crystal: ReadWrite<u32, Crystal::Register>,   /* offset 0x00 */
    Core: ReadWrite<u32, PLLCfg0::Register>,      /* offset 0x04 */
    _reserved08: u32,                             /* offset 0x08 */
    DDR0: ReadWrite<u32, PLLCfg0::Register>,      /* offset 0x0c */
    DDR1: ReadWrite<u32, PLLCfg1::Register>,      /* offset 0x10 */
    _reserved14: u32,                             /* offset 0x14 */
    _reserved18: u32,                             /* offset 0x18 */
    GE0: ReadWrite<u32, PLLCfg0::Register>,       /* offset 0x1c */
    GE1: ReadWrite<u32, PLLCfg1::Register>,       /* offset 0x20 */
    ClkSel: ReadWrite<u32, ClkSel::Register>,     /* offset 0x24 */
    DevReset: ReadWrite<u32, ResetCtl::Register>, /* offset 0x28 */
}

register_bitfields! {
    u32,
    Crystal [
        Enable OFFSET(30) NUMBITS(1) [
            Enable = 1,
            Disable = 0
        ],
        State OFFSET(29) NUMBITS(1) [
            Ready = 1
        ]
    ],
    PLLCfg0 [
        DivR OFFSET(0) NUMBITS(6) [],
        DivF OFFSET(6) NUMBITS(9) [],
        DivQ OFFSET(15) NUMBITS(3) [],
        Range OFFSET(18) NUMBITS(3) [],
        ByPass OFFSET(24) NUMBITS(1) [],
        FSE OFFSET(25) NUMBITS(1) [],
        LockRO OFFSET(31) NUMBITS(1) []
    ],
    PLLCfg1[
        Ctrl OFFSET(24) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ]
    ],
    ClkSel [
        Sel OFFSET(0) NUMBITS(1) [
            Core = 0,
            HF = 1
        ]
    ],
    ResetCtl [
        DDRCtl OFFSET(0) NUMBITS(1) [
            Reset = 0
        ],
        DDRAXI OFFSET(1) NUMBITS(1) [
            Reset = 0
        ],
        DDRAHB OFFSET(2) NUMBITS(1) [
            Reset = 0
        ],
        DDRPHY OFFSET(3) NUMBITS(1) [
            Reset = 0
        ],
        GE OFFSET(5) NUMBITS(1) [
            Reset = 0
        ] // bit 5. Not a typo.
        ]
}

// There has to be a better way but ...
// the nicest thing would be something likes this:
// fn ResetMask(m ...string)
// then match on strings like
// "ddr", "phy", whatever.
// Or possibly a varargs of enums. Whatever.
fn ResetMask(ddr: bool, axi: bool, ahb: bool, phy: bool, GE: bool) -> u32 {
    // The default is to reset nothing.
    let mut m = ReadWrite::<u32, ResetCtl::Register>::new(0x2f);
    if ddr {
        m.modify(ResetCtl::DDRCtl.val(0));
    }
    if axi {
        m.modify(ResetCtl::DDRAXI.val(0));
    }
    if ahb {
        m.modify(ResetCtl::DDRAHB.val(0));
    }
    if phy {
        m.modify(ResetCtl::DDRPHY.val(0));
    }
    if GE {
        m.modify(ResetCtl::GE.val(0));
    }
    m.get()
}

fn DefaultCore() -> u32 {
    let mut r = ReadWrite::<u32, PLLCfg0::Register>::new(0);
    r.modify(PLLCfg0::DivR.val(0));
    r.modify(PLLCfg0::DivF.val(59));
    r.modify(PLLCfg0::DivQ.val(2));
    r.modify(PLLCfg0::Range.val(4));
    r.modify(PLLCfg0::ByPass.val(0));
    r.modify(PLLCfg0::FSE.val(1));
    r.get()
}
fn DefaultDDR() -> u32 {
    let mut r = ReadWrite::<u32, PLLCfg0::Register>::new(0);
    r.modify(PLLCfg0::DivR.val(0));
    r.modify(PLLCfg0::DivF.val(55));
    r.modify(PLLCfg0::DivQ.val(2));
    r.modify(PLLCfg0::Range.val(4));
    r.modify(PLLCfg0::ByPass.val(0));
    r.modify(PLLCfg0::FSE.val(1));
    r.get()
}
fn DefaultGE() -> u32 {
    let mut r = ReadWrite::<u32, PLLCfg0::Register>::new(0);
    r.modify(PLLCfg0::DivR.val(0));
    r.modify(PLLCfg0::DivF.val(59));
    r.modify(PLLCfg0::DivQ.val(5));
    r.modify(PLLCfg0::Range.val(4));
    r.modify(PLLCfg0::ByPass.val(0));
    r.modify(PLLCfg0::FSE.val(1));
    r.get()
}

pub struct Clock<'a> {
    base: usize,
    clks: &'a mut [&'a mut dyn ClockNode],
}

impl<'a> ops::Deref for Clock<'a> {
    type Target = RegisterBlock;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr() }
    }
}

impl<'a> Driver for Clock<'a> {
    fn init(&mut self) {
        /* nothing to do. */
    }

    fn pread(&self, data: &mut [u8], _offset: usize) -> Result<usize> {
        NOT_IMPLEMENTED
    }

    fn pwrite(&mut self, data: &[u8], _offset: usize) -> Result<usize> {
        match data {
            b"on" => {
                self.clock_init();
                Ok(1)
            }
            _ => Ok(0),
        }
    }

    fn shutdown(&mut self) {}
}

//static struct prci_ctlr *prci = (void *)FU540_PRCI;

const PRCI_CORECLK_MASK: u32 = 1;
const PRCI_CORECLK_CORE_PLL: u32 = 0;
const PRCI_CORECLK_HFCLK: u32 = 1;

const PRCI_PLLCFG_LOCK: u32 = (1 << 31);
const PRCI_PLLCFG_DIVR_SHIFT: u32 = 0;
const PRCI_PLLCFG_DIVF_SHIFT: u32 = 6;
const PRCI_PLLCFG_DIVQ_SHIFT: u32 = 15;
const PRCI_PLLCFG_RANGE_SHIFT: u32 = 18;
const PRCI_PLLCFG_BYPASS_SHIFT: u32 = 24;
const PRCI_PLLCFG_FSE_SHIFT: u32 = 25;
const PRCI_PLLCFG_DIVR_MASK: u32 = (0x03f << PRCI_PLLCFG_DIVR_SHIFT);
const PRCI_PLLCFG_DIVF_MASK: u32 = (0x1ff << PRCI_PLLCFG_DIVF_SHIFT);
const PRCI_PLLCFG_DIVQ_MASK: u32 = (0x007 << PRCI_PLLCFG_DIVQ_SHIFT);
const PRCI_PLLCFG_RANGE_MASK: u32 = (0x07 << PRCI_PLLCFG_RANGE_SHIFT);
const PRCI_PLLCFG_BYPASS_MASK: u32 = (0x1 << PRCI_PLLCFG_BYPASS_SHIFT);
const PRCI_PLLCFG_FSE_MASK: u32 = (0x1 << PRCI_PLLCFG_FSE_SHIFT);

const PRCI_DDRPLLCFG1_MASK: u32 = (1 << 31);

const PRCI_GEMGXLPPLCFG1_MASK: u32 = (1 << 31);

const PRCI_CORECLKSEL_CORECLKSEL: u32 = 1;

/* Clock initialization should only be done in romstage. */

impl<'a> Clock<'a> {
    pub fn new(clks: &'a mut [&'a mut dyn ClockNode]) -> Clock<'a> {
        Clock::<'a> { base: reg::PRCI as usize, clks: clks }
    }

    /// Returns a pointer to the register block
    fn ptr(&self) -> *const RegisterBlock {
        self.base as *const _
    }

    fn init_coreclk(&self) {
        self.ClkSel.write(ClkSel::Sel::HF);

        self.Core.set(DefaultCore());

        // Spin until PLL is locked.
        while !self.Core.is_set(PLLCfg0::LockRO) {}

        self.ClkSel.write(ClkSel::Sel::Core);
    }

    fn init_pll_ddr(&self) {
        self.DDR1.write(PLLCfg1::Ctrl::Disable);

        self.DDR0.set(DefaultDDR());

        // Spin until PLL is locked.
        while !self.DDR0.is_set(PLLCfg0::LockRO) {}

        // ???? The CKE is actually bit 31, not 24 like the datasheet suggests.
        self.DDR1.set(1 << 31);
    }

    fn init_pll_ge(&self) {
        self.GE1.write(PLLCfg1::Ctrl::Disable);

        self.GE0.set(DefaultGE());

        // Spin until PLL is locked.
        while !self.GE0.is_set(PLLCfg0::LockRO) {}

        self.GE1.write(PLLCfg1::Ctrl::Enable);
    }

    fn clock_init(&mut self) {
        if is_qemu() {
            return;
        }

        // Update the peripheral clock dividers of UART, SPI and I2C to safe
        // values as we can't put them in reset before changing frequency.
        let hfclk = 1_000_000_000; // 1GHz
        for clk in self.clks.iter_mut() {
            clk.set_clock_rate(hfclk);
        }

        self.init_coreclk();
        // put DDR and ethernet in reset
        // This jams them all.
        self.DevReset.set(ResetMask(true, true, true, true, true));

        self.init_pll_ddr();

        // The following code and its comments is mostly derived from the SiFive
        // u540 bootloader.
        // https://github.com/sifive/freedom-u540-c000-bootloader

        // get DDR out of reset
        // TODO: clean this up later
        self.DevReset.set(ResetMask(false, true, true, true, true));

        // Required to get the '1 full controller clock cycle'.
        architecture::fence();

        self.DevReset.set(ResetMask(false, false, false, false, true));

        // Required to get the '1 full controller clock cycle'.
        architecture::fence();

        // These take like 16 cycles to actually propagate. We can't go sending
        // stuff before they come out of reset. So wait.
        // TODO: Add a register to read the current reset states, or DDR Control
        // device?
        for i in 0..=255 {
            architecture::nop();
        }
        self.init_pll_ge();
        self.DevReset.set(ResetMask(false, false, false, false, false));

        architecture::fence();
    }
}
