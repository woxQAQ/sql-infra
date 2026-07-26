use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use pg_completion::collect;
use pg_parser::TextSize;

struct CountingAllocator;

static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(new_size, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn large_forward_scope_stays_within_the_collection_budget() {
    let mut source = String::from("SELECT  FROM ");
    for index in 0..5_000 {
        if index != 0 {
            source.push_str(", ");
        }
        source.push_str("schema.table_");
        source.push_str(&index.to_string());
        source.push_str(" AS alias_");
        source.push_str(&index.to_string());
    }
    let point = TextSize::new(7);

    let warmup = collect(&source, point);
    assert_eq!(warmup.scope.local.relations.len(), 5_000);

    ALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
    let started = Instant::now();
    let context = collect(&source, point);
    let elapsed = started.elapsed();
    COUNTING.store(false, Ordering::Relaxed);

    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let allocated_bytes = ALLOCATED_BYTES.load(Ordering::Relaxed);
    assert_eq!(context.scope.local.relations.len(), 5_000);
    assert!(
        elapsed <= Duration::from_secs(1),
        "large completion took {elapsed:?}; allocations={allocations}; bytes={allocated_bytes}"
    );
    assert!(
        allocations <= 150_000,
        "large completion made {allocations} allocations in {elapsed:?}; bytes={allocated_bytes}"
    );
    assert!(
        allocated_bytes <= 16 * 1024 * 1024,
        "large completion allocated {allocated_bytes} bytes in {elapsed:?}; allocations={allocations}"
    );
}
