/***********************************************************************************************************************
 * Copyright (c) 2019 by the authors
 *
 * Author: André Borrmann
 * License: Apache License 2.0
 **********************************************************************************************************************/

//! # Low-level mailbox property tag interface
//!
//! This module provide the low level implementation of the mailbox property tag interface dealing with the actual
//! peripherals.
//!

extern crate alloc;
use alloc::*;

use ruspiro_cache as cache;
use ruspiro_console::*;
use ruspiro_register::define_mmio_register;

// MMIO base address for peripherals
#[cfg(feature = "ruspiro_pi3")]
const PERIPHERAL_BASE: u32 = 0x3F00_0000;

// Mailbox MMIO base address
const MAILBOX_BASE: u32 = PERIPHERAL_BASE + 0x0000_B880;

/// Definition of the different message stats/types used in the mailbox interface
#[repr(u32)]
#[derive(Copy, Clone)]
#[allow(dead_code)]
pub enum MessageState {
    /// All outgoing messages are of the request type
    Request = 0x0,
    /// If the message has been successfull processed by the receiver
    ResponseOk = 0x8000_0000,
    /// If the message hs not been successfully or just partly successfully processed by the receiver
    ResponseError = 0x8000_0001,
}

/// Definition of the different mailbox channels to be used for communication
#[repr(u8)]
#[derive(Copy, Clone)]
#[allow(dead_code)]
pub enum MailboxChannel {
    /// Power management channel
    PowerMgmt = 0x0,
    /// Framebuffer channel (shall not be used)
    FrameBuffer = 0x1,
    /// Virtual UART channel
    VirtualUart = 0x2,
    /// Property tag channel to send from ARM to VideoCore
    PropertyTagsVc = 0x8,
    /// Property tag channel to send from VideoCore to ARM
    PropertyTagsArm = 0x9,
}

/// Trait each mailbox property tag message need to implement.
/// It is used as trait bound in the functions sending messages through the mailbox.
pub trait MailboxMessage {
    fn get_state(&self) -> u32;
}

/// Type alias for Results of the functions in this module
pub type MailboxResult<T> = Result<T, &'static str>;

/// Function to send a specific message to the mailbox channel given
/// The mailbox interface does update the memory location of the message send. Therefor the function returns
/// Ok with the updated message in case of a success
// never inline, if inlined the compiler seem to mess up something and no mailbox call succeeds
//#[inline(never)]
pub(crate) fn send_message<T: MailboxMessage>(
    channel: MailboxChannel,
    mut message: T,
) -> MailboxResult<T> {
    let msg_ptr: *mut T = &mut message;
    let msg_ptr_uncached: u32 = (msg_ptr as u32) | 0xC000_0000;

    cache::cleaninvalidate();
    write(channel, msg_ptr_uncached)?;
    read(channel)?;
    cache::cleaninvalidate();
    // at this point the property tag message memory has been changed under the hood
    // that rust is not aware of, so optimizations might do things that will loose this fact
    // so read this memory location back into the corresponding message type T
    let result_ptr = (msg_ptr_uncached ^ 0xC000_0000) as *mut T;
    //println!("result/origin ptr: {:X} / {:X}", result_ptr as usize, msg_ptr as usize);
    let result = unsafe { core::ptr::read_volatile(result_ptr) };
    let result_state = result.get_state();
    if result_state == MessageState::ResponseOk as u32 {
        Ok(result)
    } else {
        Err("unable to send mailbox property tag message.")
    }
}

define_mmio_register! [
    MAILBOX0_READ<ReadOnly<u32>@(MAILBOX_BASE + 0x00)>,
    MAILBOX0_STATUS<ReadOnly<u32>@(MAILBOX_BASE + 0x18)>,
    MAILBOX1_WRITE<WriteOnly<u32>@(MAILBOX_BASE + 0x20)>,
    MAILBOX1_STATUS<ReadOnly<u32>@(MAILBOX_BASE + 0x38)>
];

const MAILBOX_FULL: u32 = 0x8000_0000; // status register value if the mailbox is already full
const MAILBOX_EMPTY: u32 = 0x4000_0000; // status register value if the mailbox is empty

#[inline(always)]
fn read(channel: MailboxChannel) -> MailboxResult<u32> {
    loop {
        while (MAILBOX0_STATUS::Register.get() & MAILBOX_EMPTY) != 0x0 {}
        let data = MAILBOX0_READ::Register.get();
        if (data & 0xF) == channel as u32 {
            return Ok(data & 0xFFFF_FFF0);
        }
    }
}

#[inline(always)]
fn write(channel: MailboxChannel, data: u32) -> MailboxResult<()> {
    while (MAILBOX1_STATUS::Register.get() & MAILBOX_FULL) != 0x0 {}
    let value = (data & 0xFFFF_FFF0) | ((channel as u8) & 0xF) as u32;
    MAILBOX1_WRITE::Register.set(value);
    Ok(())
}
