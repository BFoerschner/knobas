//! A `SecretText` wipes its bytes before its buffer is freed (ADR-0056: a
//! secret never rests). A binary of its own, because it swaps the global
//! allocator: the allocator copies one armed block's bytes inside
//! `dealloc`, while the memory is still live, so the test reads what the
//! drop left without touching freed memory (a Fable ruling in the final
//! suite, 2026-11-22: a code reading is not a test).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use knobas_source::passwords::SecretText;

/// The most bytes a captured block keeps.
const ROOM: usize = 64;

/// System's allocator, which copies the block at [`ARMED`] when it is freed.
struct Witness;

/// The address of the one block to copy at its `dealloc`; 0 for none.
static ARMED: AtomicUsize = AtomicUsize::new(0);
/// Set once the armed block was copied into [`SEEN`].
static CAUGHT: AtomicBool = AtomicBool::new(false);
static SEEN_LEN: AtomicUsize = AtomicUsize::new(0);
/// Written only inside `dealloc` for the armed block, read after `CAUGHT`.
static mut SEEN: [u8; ROOM] = [0; ROOM];
/// One armed block at a time: the tests in this binary take turns.
static TURN: Mutex<()> = Mutex::new(());

// SAFETY: every call is passed to `System` unchanged; `dealloc` only reads the
// block before handing it back, and writes `SEEN` for the one armed address,
// which a single test at a time arms (under `TURN`).
unsafe impl GlobalAlloc for Witness {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr as usize == ARMED.load(Ordering::SeqCst) {
            let len = layout.size().min(ROOM);
            unsafe {
                std::ptr::copy_nonoverlapping(ptr, std::ptr::addr_of_mut!(SEEN).cast::<u8>(), len);
            }
            SEEN_LEN.store(len, Ordering::SeqCst);
            ARMED.store(0, Ordering::SeqCst);
            CAUGHT.store(true, Ordering::SeqCst);
        }
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Witness = Witness;

/// The bytes the block at `address` held when it was freed by `drop`.
fn freed_bytes(address: usize, drop: impl FnOnce()) -> Vec<u8> {
    CAUGHT.store(false, Ordering::SeqCst);
    ARMED.store(address, Ordering::SeqCst);
    drop();
    assert!(CAUGHT.load(Ordering::SeqCst), "the armed block was freed");
    let len = SEEN_LEN.load(Ordering::SeqCst);
    // SAFETY: `CAUGHT` is set after `dealloc` wrote `SEEN`, and nothing writes
    // it again until the next arming, which `TURN` keeps out.
    unsafe { std::slice::from_raw_parts(std::ptr::addr_of!(SEEN).cast::<u8>(), len).to_vec() }
}

#[test]
fn a_secret_is_wiped_before_its_buffer_is_freed() {
    let _turn = TURN.lock().unwrap();
    let secret = SecretText::new(String::from("hunter2-hunter2"));
    let address = secret.expose().as_ptr() as usize;
    let bytes = freed_bytes(address, move || drop(secret));
    assert!(!bytes.is_empty());
    assert!(bytes.iter().all(|&b| b == 0), "freed holding {bytes:?}");
}

/// Without this the test above proves nothing: a plain `String` is freed
/// holding its text, so the witness does read what a drop leaves.
#[test]
fn a_plain_string_is_freed_holding_its_text() {
    let _turn = TURN.lock().unwrap();
    let plain = String::from("hunter2-hunter2");
    let address = plain.as_ptr() as usize;
    let bytes = freed_bytes(address, move || drop(plain));
    assert_eq!(bytes, b"hunter2-hunter2");
}
