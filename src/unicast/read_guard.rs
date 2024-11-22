use std::ptr;
use std::ptr::NonNull;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::mem::ManuallyDrop;
use branch_hints::unlikely;
use std::sync::atomic::{fence, Ordering};
use crate::unicast::block::{Block, BLOCK_SIZE};
use crate::unicast::spmc::Queue;

/// Owning [Queue] message wrapper.
/// 
/// # Design choices
/// 
/// We could just always return a value, but that would require mempcy for
/// every read message. With ReadGuard we basically give you a reference, that 
/// can be [take]n. 
pub struct ReadGuard<'a, T>{
    pub(crate) value: NonNull<T>,
    pub(crate) block: &'a Block<T>,
    // We own T
    pub(crate) phantom_data: PhantomData<T>
}

impl<'a, T> ReadGuard<'a, T>{
    #[inline]
    fn mark_readed(&mut self) {
        if unlikely(self.block.read_succ.fetch_add(1, Ordering::Release) == BLOCK_SIZE-1) {
            // See Arc::drop implementation, for this fence rationale.
            fence(Ordering::Acquire);
            unsafe{self.block.dealloc_destructed_mem()};
        }
    }
    
    #[inline]
    pub fn take(self) -> T {
        let mut this = ManuallyDrop::new(self);
        let value = unsafe{ this.value.read() };
        this.mark_readed();
        value
    }
}

impl<'a, T> Deref for ReadGuard<'a, T>{
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe{ self.value.as_ref() }
    }
}

impl<'a, T> DerefMut for ReadGuard<'a, T>{
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe{ self.value.as_mut() }
    }
}

impl<'a, T> Drop for ReadGuard<'a, T>{
    #[inline]
    fn drop(&mut self) {
        // 1. Drop value
        unsafe {
            ptr::drop_in_place(self.value.as_ptr());
        }
        
        // 2. Drop block's mem, if needed.
        self.mark_readed();
    }
}

/// Same as [ReadGuard], but for session.
pub struct ReadSessionGuard<'a, T>{
    pub(crate) value: NonNull<T>,
    pub(crate) read_succ: &'a mut usize,
    // We own T
    pub(crate) phantom_data: PhantomData<T>
}

impl<'a, T> ReadSessionGuard<'a, T> {
    #[inline]
    pub fn take(self) -> T {
        let mut this = ManuallyDrop::new(self);
        let value = unsafe{ this.value.read() };
        *this.read_succ += 1;
        value
    }
}

impl<'a, T> Deref for ReadSessionGuard<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe{ self.value.as_ref() }
    }
}

impl<'a, T> DerefMut for ReadSessionGuard<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe{ self.value.as_mut() }
    }
}

impl<'a, T> Drop for ReadSessionGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.value.as_ptr());
        }
        *self.read_succ += 1;
    }
}

// *read_n have unpredicted performance in spmc. Hide it for now. 
/*
pub struct SliceReadGuard<'a, T>{
    pub(crate) start: NonNull<T>,
    pub(crate) len: usize,
    pub(crate) block: &'a Block<T>,
    // We own T
    pub(crate) phantom_data: PhantomData<T>
}

impl<'a, T> SliceReadGuard<'a, T>{
    #[inline(always)]
    fn mark_readed(&mut self) {
        if unlikely(self.block.read_succ.fetch_add(self.len, Ordering::Release) == BLOCK_SIZE-self.len) {
            // See Arc::drop implementation, for this fence rationale.
            fence(Ordering::Acquire);
            unsafe{self.block.dealloc_destructed_mem()};
        }
    }    
    
    // TODO: take() -> impl Iterator<Item = T>
}

impl<'a, T> Deref for SliceReadGuard<'a, T>{
    type Target = [T];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe{
            std::slice::from_raw_parts(
                self.start.as_ptr(),
                self.len
            )
        }
    }
}

impl<'a, T> Drop for SliceReadGuard<'a, T>{
    #[inline(always)]
    fn drop(&mut self) {
        // 1. Drop values
        if mem::needs_drop::<T>(){
            for i in 0..self.len {
                unsafe {
                    ptr::drop_in_place(self.start.as_ptr().add(i));
                }
            }
        }
        
        // 2. Drop block's mem, if needed.
        self.mark_readed();
    }
}

pub struct SliceReadSessionGuard<'a, T>{
    pub(crate) start: NonNull<T>,
    pub(crate) len: usize,
    pub(crate) read_succ: &'a mut usize,
    // We own T
    pub(crate) phantom_data: PhantomData<T>
}

impl<'a, T> Deref for SliceReadSessionGuard<'a, T> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe{
            std::slice::from_raw_parts(
                self.start.as_ptr(),
                self.len
            )
        }
    }
}

impl<'a, T> Drop for SliceReadSessionGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        // 1. Drop values
        if mem::needs_drop::<T>(){
            for i in 0..self.len {
                unsafe {
                    ptr::drop_in_place(self.start.as_ptr().add(i));
                }
            }
        }        
        
        *self.read_succ += self.len;
    }
}
*/
