lockfree_broadcast_queue

# CRATE NAME

An mpsc[^mpsc]/spmc[^spmc] lock-free broadcast[^broadcast] queue. Can be used as a channel as well.

[^mpsc]: Multi-producer multi-consumer.

[^spmc]: Single-producer multi-consumer.

[^broadcast]: Broadcast means that each consumer gets every message sent on the channel,
from the moment of subscription.

* Lock-free consumers without overhead[^lockfree_overhead].
* Mpsc lock-free producers, which write simultaneously.
* Spmc ordered. Mpsc ordered within writer messages[^mpsc_order].
* Unbounded dynamic size.
* Shared queue. All readers and writers use the same queue, without duplications.

Blazingly fast reads. The consumer basically reads a plain slice of data, then does an atomic read that will define the next slice.

[^lockfree_overhead]: In compare to traditional lock techniques with Mutex.

[^mpsc_order]: This means that each message written by writer,
will be in the same order against each other. 
But between them, messages from other threads **may** appear.
If write calls will be synchronized - all messages will be ordered by that "synchronization order".

# Example

TODO

# How it works

## Reading

It is based on [rc_event_queue](https://crates.io/crates/rc_event_queue) idea of reader counters. 
Queue represented as an atomic single-linked list of blocks. Each block have atomic use counter (like Arc). Each block pointed by "next" in list node have +1 use count as well.

```rust
struct Block{
    ..
    next     : AtomicPtr<Block>,
    use_count: AtomicUsize,    
    mem      : [T; BLOCK_SIZE]
}
```

```rust
struct Reader{
    // This prevents block and the rest of the list AFTER it from disappearing.
    block: BlockArc,   
    index: usize,
    len  : usize,       // last known Block::len value. Re-load each time index==len.
}
```
When reader enters next block, it increases it's counter, and decreases old one. Then it reads block's atomic len - and it is safe to read from block start to that len. 

Only the front block can down counter to 0. This is because it is the only block,
that does not pointed by "next" in list. Even if there will be no readers in the block in the middle of the list - it still will have `use_count = 1`, because it is pointed by prev block in list.
So, when the last reader left front block, it use counter will drop to 0 - and it will be dropped.

And since queue **CAN NOT** in any way dispose blocks in the middle of the list, this means that "next" pointer can not be changed in the middle of the list at all. And only the latest/back block can change its "next" pointer from NULL to some real block, when new block pushed to the list.

 This guarantees that read next block will not be disposed, until we hold arc pointer to it's previous block. Which means that no additional synchronization needed when reader moves to the next block. It can just atomically read "next" pointer - it will always be valid or NULL.

## Writing

Simplified version of pushing value from mpmc writer: 

```rust
struct Block {
    ..
    packed: AtomicU64       // occupied_len:u32 + writers:u32
    len   : AtomicUsize,
    mem   : [T; BLOCK_SIZE]
}

// 1. occupied_len += 1 and writers += 1 in one go. 
let Packed{ occupied_len, .. } = block.packed.fetch_add(
    Packed{ occupied_len: 1, writers: 1 }.into(),
    Ordering::AcqRel
).into();

if occupied_len >= BLOCK_SIZE {
    // Put counters back.
    ..
    return Err();   // here we allocate next block, etc...
}

// 2. Actually write value
block.mem[occupied_len].write(value);

// 3. writers -= 1
let Packed{ occupied_len, writers } = block.packed.fetch_sub(
    Packed{ occupied_len: 0, writers: 1 }.into(),
    Ordering::AcqRel
).into();

// 4. If writers == 0, means all simultaneous writes over, 
//    and occupied_len can be set as real len.
if writers == 1 {
    // We can't just len.store(occupied_len).
    // It is possible that there is other thread
    // that just finished writing AFTER our position in mem,
    // and updated len with HIGHER value.
    // So we want highest value, instead.
    block.len.fetch_max(occupied_len, Ordering::Release);
}
```

In single-threaded push() we would just `block.mem[len++] = value` and then `block.len.store(len)`.
But in multithreaded environment we can't just update `block.len` with actual value, since some values
BEFORE len can be still in progress of writing. So we need separate len, and writer counter to know when all finish writes and `len` can be updated for readers.

There is a case when writers keep constantly writing, and it could look like that writer counter NEVER reach 0. But! Since we have per-block counter - as soon as writers travel to the next one - counter WILL drop to 0.
So we just use reasonably (like 4096) sized blocks - if writers write constantly - it will be depleted and changed fast.