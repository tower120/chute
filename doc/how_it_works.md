## How it works

![Queue illustration](img/mpmc_white.png)

### Reading

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

### Writing

There are several algorithms developed as chute evolved. You may just skip for the last one.

#### Write counters (v0.1.x)

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

#### Atomic bitblocks (>=v0.2.0)

This algorithm solves a "never reaching 0" problem of a previous one. And it is faster as well. 

Each Block now have `bit_blocks` array of `AtomicU64`:
```rust
struct Block {
    ..
    len: AtomicUsize,
    bit_blocks: [AtomicU64; BLOCK_SIZE/64]
}
```
```rust
// Get index.
let occupied_len = self.len.fetch_add(1, Ordering::AcqRel);

if unlikely(occupied_len >= BLOCK_SIZE) {
    return Err(value);
}

// Actually save value.
let index = occupied_len;
unsafe{
    let mem = self.mem().cast_mut();
    mem.add(index).write(value);
}

// Update bitblock, indicating that value is ready to read.
{
    let bit_block_index = index / 64;
    let bit_index = index % 64;
    
    let bitmask = 1 << bit_index;
    let atomic_block = unsafe{ self.bit_blocks.get_unchecked(bit_block_index) };
    atomic_block.fetch_or(bitmask, Ordering::Release);
}
```
Now on the reader side we iterate over `bit_blocks` and get `trailing_ones()` from each
bitblock, to form a len:
```rust
let bit_block = self.block.bit_blocks[self.bitblock_index].load(Ordering::Acquire);
let new_len = self.bitblock_index*64 + bit_block.trailing_ones() as usize;
```
We need to move to the next bit-block once every 64 read messages, so this is virtually
unnoticable from performance perspective.

On result - we have 30% better overall performance, while getting our 
messages ASAP and still keeping each writer's order. At a price of insignificant 
memory overhead of a 1 bit per message.