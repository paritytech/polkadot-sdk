mod bump;

#[cfg(not(any(feature = "std", feature = "no-allocator")))]
#[global_allocator]
static mut ALLOC: bump::BumpAllocator = bump::BumpAllocator {};
