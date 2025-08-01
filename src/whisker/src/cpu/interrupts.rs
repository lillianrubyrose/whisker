//! implementation of interrupts from external sources

use std::sync::mpsc::{self, Receiver, Sender};

use crate::cpu::WhiskerCpu;
use crate::log;
use crate::ty::TrapIdx;

#[derive(Debug)]
pub enum InterruptEvent {
	UartInterrupt,
}

#[derive(Debug)]
pub struct InterruptController {
	int_channel: Receiver<InterruptEvent>,
}

impl InterruptController {
	pub fn new() -> (Sender<InterruptEvent>, Self) {
		let (tx, rx) = mpsc::channel();

		(tx, Self { int_channel: rx })
	}
}

impl WhiskerCpu {
	pub fn poll_interrupt_controller(&mut self) -> bool {
		log!(self, "checking interrupt controller");
		match self.interrupt_controller.int_channel.try_recv() {
			// nothing available, wait
			Err(mpsc::TryRecvError::Empty) => false,
			Err(mpsc::TryRecvError::Disconnected) => panic!("interrupt controller senders disconnected"),
			Ok(event) => match event {
				InterruptEvent::UartInterrupt => {
					self.request_trap(TrapIdx::MACHINE_EXTERNAL_INTERRUPT, 0);
					true
				}
			},
		}
	}
}
