#[cfg(test)]
extern crate scoped_threadpool;

use alloc::boxed::Box;
use alloc::fmt;
use core::mem;
use core::num::NonZeroUsize;
use core::ptr::{self, NonNull};

#[cfg(all(test, not(feature = "hashbrown")))]
extern crate std;

extern crate alloc;

/// A node in the doubly linked list.
///
/// Contains a value and pointers to the previous and next entries.
/// This structure is not meant to be used directly by users of the `List`.
pub struct Entry<T> {
    /// The value stored in this entry. Uses MaybeUninit to allow for sigil nodes.
    val: mem::MaybeUninit<T>,
    /// Pointer to the previous entry in the list.
    prev: *mut Entry<T>,
    /// Pointer to the next entry in the list.
    next: *mut Entry<T>,
}

impl<T> Entry<T> {
    /// Creates a new entry with the given value.
    fn new(val: T) -> Self {
        Entry {
            val: mem::MaybeUninit::new(val),
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
        }
    }

    /// Creates a new sigil (sentinel) entry without initializing the value.
    ///
    /// Sigil entries are used as head and tail markers in the list.
    fn new_sigil() -> Self {
        Entry {
            val: mem::MaybeUninit::uninit(),
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
        }
    }

    /// Safely extracts the value from this entry.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it assumes the value is initialized.
    /// Should only be called on non-sigil nodes.
    pub unsafe fn get_value(&self) -> &T {
        self.val.assume_init_ref()
    }

    /// Safely extracts a mutable reference to the value from this entry.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it assumes the value is initialized.
    /// Should only be called on non-sigil nodes.
    pub unsafe fn get_value_mut(&mut self) -> &mut T {
        self.val.assume_init_mut()
    }
}

/// A doubly linked list implementation with fixed capacity.
///
/// This list maintains a fixed capacity specified at creation time and provides
/// O(1) operations for adding, removing, and updating elements. The list uses
/// sentinel nodes (sigils) at the head and tail to simplify operations.
///
/// # Examples
///
/// ```ignore
/// use cache_rs::list::List;
/// use core::num::NonZeroUsize;
///
/// let mut list = List::new(NonZeroUsize::new(3).unwrap());
///
/// // Add elements to the list
/// let node1 = list.add(10).unwrap();
/// let node2 = list.add(20).unwrap();
///
/// // Update an element
/// unsafe {
///     list.update(node1, 15, false);
/// }
/// ```
pub struct List<T> {
    /// Maximum number of items the list can hold.
    cap: NonZeroUsize,
    /// Current number of items in the list.
    len: usize,
    /// Pointer to the head sentinel node.
    head: *mut Entry<T>,
    /// Pointer to the tail sentinel node.
    tail: *mut Entry<T>,
}

impl<T> List<T> {
    /// Creates a new List that holds at most `cap` items.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use cache_rs::list::List;
    /// use core::num::NonZeroUsize;
    ///
    /// let list: List<u32> = List::new(NonZeroUsize::new(5).unwrap());
    /// assert_eq!(list.cap().get(), 5);
    /// ```
    pub fn new(cap: NonZeroUsize) -> List<T> {
        List::construct(cap)
    }
}

impl<T> List<T> {
    /// Creates a new list with the given capacity.
    ///
    /// This method sets up the sentinel nodes and links them together.
    fn construct(cap: NonZeroUsize) -> List<T> {
        let head = Box::into_raw(Box::new(Entry::new_sigil()));
        let tail = Box::into_raw(Box::new(Entry::new_sigil()));

        let cache = List {
            cap,
            len: 0,
            head,
            tail,
        };

        unsafe {
            // SAFETY: head and tail are newly allocated and valid pointers
            (*cache.head).next = cache.tail;
            (*cache.tail).prev = cache.head;
        }

        cache
    }

    /// Returns the maximum number of items the list can hold.
    pub fn cap(&self) -> NonZeroUsize {
        self.cap
    }

    /// Returns the current number of items in the list.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the list contains no items.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns true if the list is at capacity.
    #[allow(dead_code)]
    pub fn is_full(&self) -> bool {
        self.len == self.cap.get()
    }

    /// Removes the first (most recently added) item from the list.
    ///
    /// Returns the removed entry if the list is not empty.
    ///
    /// # Safety
    ///
    /// This method is safe because it properly manages all raw pointer operations
    /// and ensures no memory leaks or dangling pointers.
    pub fn remove_first(&mut self) -> Option<Box<Entry<T>>> {
        if self.is_empty() {
            return None;
        }
        // SAFETY: Both head and tail are valid pointers initialized in `construct`,
        // and we know the list is not empty, so there's at least one element between them
        let next = unsafe { (*self.head).next };
        if next != self.tail {
            unsafe {
                self._detach(next);
            }
            self.len -= 1;
            // SAFETY: next is a valid pointer as we just detached it
            unsafe { Some(Box::from_raw(next)) }
        } else {
            None
        }
    }

    /// Removes the last (least recently added) item from the list.
    ///
    /// Returns the removed entry if the list is not empty.
    ///
    /// # Safety
    ///
    /// This method is safe because it properly manages all raw pointer operations
    /// and ensures no memory leaks or dangling pointers.
    pub fn remove_last(&mut self) -> Option<Box<Entry<T>>> {
        if self.is_empty() {
            return None;
        }
        // SAFETY: Both head and tail are valid pointers initialized in `construct`,
        // and we know the list is not empty, so there's at least one element between them
        let prev = unsafe { (*self.tail).prev };
        if prev != self.head {
            unsafe {
                self._detach(prev);
            }
            self.len -= 1;
            // SAFETY: prev is a valid pointer as we just detached it
            unsafe { Some(Box::from_raw(prev)) }
        } else {
            None
        }
    }

    /// Detaches a node from the list and returns it as a Box.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it takes a raw pointer parameter.
    /// The caller must ensure that `node` is a valid pointer to a node in the list
    /// (not null, not freed, and actually part of this list).
    pub unsafe fn remove(&mut self, node: *mut Entry<T>) -> Option<Box<Entry<T>>> {
        if self.is_empty() || node.is_null() || node == self.head || node == self.tail {
            return None;
        }

        unsafe {
            // SAFETY: Caller guarantees node is valid and part of this list
            // Detach the node from the list
            self._detach(node);
            self.len -= 1;

            // Return the specified node as a Box, not the first node
            Some(Box::from_raw(node))
        }
    }

    /// Detaches a node from the list without deallocating it.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it dereferences raw pointers.
    /// The caller must ensure that `node` is a valid pointer to a node in the list
    /// (not null, not freed, and actually part of this list).
    unsafe fn _detach(&mut self, node: *mut Entry<T>) {
        // SAFETY: The caller guarantees that node is a valid entry in the list,
        // which means its prev and next pointers are also valid entries.
        unsafe {
            (*(*node).prev).next = (*node).next;
            (*(*node).next).prev = (*node).prev;
        }
    }

    /// Attaches a node after the head sentinel node.
    ///
    /// This effectively makes the node the first item in the list.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it dereferences raw pointers.
    /// The caller must ensure that `node` is a valid pointer to a node that is
    /// not already in the list (e.g., newly allocated or previously detached).
    pub unsafe fn attach(&mut self, node: *mut Entry<T>) {
        // SAFETY: head is a valid pointer initialized in `construct`,
        // and the caller guarantees that node is a valid entry not already in the list
        (*node).next = (*self.head).next;
        (*node).prev = self.head;
        (*self.head).next = node;
        (*(*node).next).prev = node;
    }

    /// Attaches a node before the tail sentinel node.
    ///
    /// This effectively makes the node the last item in the list.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it dereferences raw pointers.
    /// The caller must ensure that `node` is a valid pointer to a node that is
    /// not already in the list (e.g., newly allocated or previously detached).
    #[allow(dead_code)]
    pub unsafe fn attach_last(&mut self, node: *mut Entry<T>) {
        // SAFETY: tail is a valid pointer initialized in `construct`,
        // and the caller guarantees that node is a valid entry not already in the list
        (*node).next = self.tail;
        (*node).prev = (*self.tail).prev;
        (*self.tail).prev = node;
        (*(*node).prev).next = node;
    }

    /// Attaches a node from another list after the head sentinel node.
    ///
    /// This method should be used when moving a node between different lists.
    /// It increments the length of this list since it's gaining a node.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it dereferences raw pointers.
    /// The caller must ensure that `node` is a valid pointer to a node that is
    /// not already in this list.
    pub unsafe fn attach_from_other_list(&mut self, node: *mut Entry<T>) {
        self.attach(node);
        self.len += 1;
    }

    /// Attaches a node from another list before the tail sentinel node.
    ///
    /// This method should be used when moving a node between different lists.
    /// It increments the length of this list since it's gaining a node.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it dereferences raw pointers.
    /// The caller must ensure that `node` is a valid pointer to a node that is
    /// not already in this list.
    #[allow(dead_code)]
    pub unsafe fn attach_last_from_other_list(&mut self, node: *mut Entry<T>) {
        self.attach_last(node);
        self.len += 1;
    }

    /// Moves a node to the front of the list (after the head sentinel).
    ///
    /// # Safety
    ///
    /// This function is unsafe because it dereferences raw pointers.
    /// The caller must ensure that `node` points to a valid entry in the list.
    pub unsafe fn move_to_front(&mut self, node: *mut Entry<T>) {
        if node.is_null() || node == self.head || node == self.tail {
            return;
        }

        // If the node is already the first item, do nothing
        if (*self.head).next == node {
            return;
        }

        // Detach the node from its current position
        self._detach(node);

        // Reattach at the front
        self.attach(node);
    }

    /// Adds a value to the front of the list.
    ///
    /// Returns a pointer to the newly created entry, or None if the list is full.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use cache_rs::list::List;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut list = List::new(NonZeroUsize::new(2).unwrap());
    /// let node1 = list.add(10).unwrap();
    /// let node2 = list.add(20).unwrap();
    ///
    /// // List is now full
    /// assert!(list.add(30).is_none());
    /// ```
    ///
    /// # Safety
    ///
    /// This method is safe because it properly manages all raw pointer operations
    /// and ensures no memory leaks or dangling pointers.
    pub fn add(&mut self, v: T) -> Option<*mut Entry<T>> {
        if self.len == self.cap().get() {
            return None;
        }
        // SAFETY: Box::into_raw creates a valid raw pointer and we're using NonNull
        // to assert its non-nullness
        let node = unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(Entry::new(v)))) };
        // SAFETY: node is a newly allocated entry that is not part of any list yet
        unsafe { self.attach(node.as_ptr()) };
        self.len += 1;
        Some(node.as_ptr())
    }

    /// Adds a value to the front of the list, bypassing the capacity check.
    ///
    /// This method allows the list to temporarily exceed its capacity.
    /// It should be used carefully and only when the total cache capacity allows it.
    ///
    /// Returns a pointer to the newly created entry.
    ///
    /// # Safety
    ///
    /// This method is safe because it properly manages all raw pointer operations
    /// and ensures no memory leaks or dangling pointers. However, the caller must
    /// ensure that bypassing the capacity is appropriate.
    pub fn add_unchecked(&mut self, v: T) -> *mut Entry<T> {
        // SAFETY: Box::into_raw creates a valid raw pointer and we're using NonNull
        // to assert its non-nullness
        let node = unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(Entry::new(v)))) };
        // SAFETY: node is a newly allocated entry that is not part of any list yet
        unsafe { self.attach(node.as_ptr()) };
        self.len += 1;
        node.as_ptr()
    }

    /// Updates the value of the given node.
    ///
    /// Returns a tuple containing:
    /// - The old value (if `capturing` is true)
    /// - A boolean indicating whether the update was successful
    ///
    /// # Safety
    ///
    /// This function is unsafe because it dereferences a raw pointer. The caller must ensure
    /// that the `node` pointer is valid and points to an `Entry` within the list.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use cache_rs::list::List;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut list = List::new(NonZeroUsize::new(2).unwrap());
    /// let node = list.add(10).unwrap();
    ///
    /// // Update and capture the old value
    /// let (old_val, success) = unsafe { list.update(node, 99, true) };
    /// assert_eq!(old_val, Some(10));
    /// assert!(success);
    ///
    /// // Update without capturing the old value
    /// let (old_val, success) = unsafe { list.update(node, 123, false) };
    /// assert_eq!(old_val, None);
    /// assert!(success);
    /// ```
    pub unsafe fn update(
        &mut self,
        node: *mut Entry<T>,
        v: T,
        capturing: bool,
    ) -> (Option<T>, bool) {
        if node.is_null() {
            return (None, false);
        }
        let old_val =
            unsafe { mem::replace(&mut (*node).val, mem::MaybeUninit::new(v)).assume_init() };

        match capturing {
            true => (Some(old_val), true),
            false => (None, true),
        }
    }

    /// Gets an immutable reference to the value stored in the entry.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it dereferences a raw pointer.
    /// The caller must ensure that the `node` pointer is valid and points to a
    /// non-sigil `Entry` within the list.
    #[allow(dead_code)]
    pub unsafe fn get_value(&self, node: *mut Entry<T>) -> Option<&T> {
        if node.is_null() || node == self.head || node == self.tail {
            None
        } else {
            Some((*node).get_value())
        }
    }

    /// Gets a mutable reference to the value stored in the entry.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it dereferences a raw pointer.
    /// The caller must ensure that the `node` pointer is valid and points to a
    /// non-sigil `Entry` within the list.
    #[allow(dead_code)]
    pub unsafe fn get_value_mut(&mut self, node: *mut Entry<T>) -> Option<&mut T> {
        if node.is_null() || node == self.head || node == self.tail {
            None
        } else {
            Some((*node).get_value_mut())
        }
    }

    /// Clears the list, removing all entries.
    pub fn clear(&mut self) {
        while self.remove_first().is_some() {}
    }
}

impl<T> Drop for List<T> {
    /// Cleans up all resources used by the list.
    ///
    /// This includes:
    /// 1. Removing and deallocating all regular entries
    /// 2. Deallocating the sentinel nodes
    fn drop(&mut self) {
        // Remove all entries
        self.clear();

        // Free the sentinel nodes
        // SAFETY: head and tail are valid pointers initialized in `construct` and never modified
        // except to be replaced with null when freed. We check for null here as an extra precaution.
        unsafe {
            if !self.head.is_null() {
                let _ = Box::from_raw(self.head);
                self.head = ptr::null_mut();
            }
            if !self.tail.is_null() {
                let _ = Box::from_raw(self.tail);
                self.tail = ptr::null_mut();
            }
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for List<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("List")
            .field("capacity", &self.cap)
            .field("length", &self.len)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn test_construct_and_cap() {
        let list = List::<u32>::new(NonZeroUsize::new(3).unwrap());
        assert_eq!(list.cap().get(), 3);
        assert_eq!(list.len, 0);
        assert!(!list.head.is_null());
        assert!(!list.tail.is_null());
    }

    #[test]
    fn test_add_items() {
        let mut list = List::<u32>::new(NonZeroUsize::new(2).unwrap());
        let node1 = list.add(10).unwrap();
        let node2 = list.add(20).unwrap();
        assert_eq!(list.len, 2);
        assert_ne!(node1, node2);
        // Should fail to add when at capacity
        assert!(list.add(30).is_none());
        assert_eq!(list.len, 2);
    }

    #[test]
    fn test_update_item() {
        let mut list = List::<u32>::new(NonZeroUsize::new(2).unwrap());
        let node = list.add(10).unwrap();
        let (old_val, success) = unsafe { list.update(node, 99, true) };
        assert_eq!(old_val, Some(10));
        assert!(success);
        let (old_val2, success2) = unsafe { list.update(node, 123, false) };
        assert_eq!(old_val2, None);
        assert!(success2);
    }

    #[test]
    fn test_get_value() {
        let mut list = List::<String>::new(NonZeroUsize::new(3).unwrap());
        let node = list.add(String::from("test")).unwrap();

        unsafe {
            let value = list.get_value(node).unwrap();
            assert_eq!(value, "test");

            let value_mut = list.get_value_mut(node).unwrap();
            value_mut.push_str("_modified");

            let value_after = list.get_value(node).unwrap();
            assert_eq!(value_after, "test_modified");

            // update the full value
            let value_mut = list.get_value_mut(node).unwrap();
            *value_mut = String::from("new_value");
            let value_after = list.get_value(node).unwrap();
            assert_eq!(value_after, "new_value");
        }
    }

    #[test]
    fn test_remove_first_and_last() {
        let mut list = List::<u32>::new(NonZeroUsize::new(3).unwrap());

        // Test removing from empty list
        assert!(list.remove_first().is_none());
        assert!(list.remove_last().is_none());

        // Add items
        let _node1 = list.add(10).unwrap();
        let _node2 = list.add(20).unwrap();
        let _node3 = list.add(30).unwrap();
        assert_eq!(list.len(), 3);

        // Remove first item (should be 30, since we add to front)
        let first = list.remove_first().unwrap();
        assert_eq!(unsafe { first.val.assume_init() }, 30);
        assert_eq!(list.len(), 2);

        // Remove last item (should be 10)
        let last = list.remove_last().unwrap();
        assert_eq!(unsafe { last.val.assume_init() }, 10);
        assert_eq!(list.len(), 1);

        // Check remaining item (should be 20)
        let last_remaining = list.remove_first().unwrap();
        assert_eq!(unsafe { last_remaining.val.assume_init() }, 20);
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_move_to_front() {
        let mut list = List::<u32>::new(NonZeroUsize::new(3).unwrap());

        // Add items: front->30->20->10->back
        let node1 = list.add(10).unwrap();
        let _node2 = list.add(20).unwrap();
        let _node3 = list.add(30).unwrap();

        // Move the last item (10) to front: front->10->30->20->back
        unsafe {
            list.move_to_front(node1);
        }

        // Check order by removing
        let first = list.remove_first().unwrap();
        assert_eq!(unsafe { first.val.assume_init() }, 10);

        let second = list.remove_first().unwrap();
        assert_eq!(unsafe { second.val.assume_init() }, 30);

        let third = list.remove_first().unwrap();
        assert_eq!(unsafe { third.val.assume_init() }, 20);
    }

    #[test]
    fn test_clear() {
        let mut list = List::<u32>::new(NonZeroUsize::new(3).unwrap());

        // Add items
        let _node1 = list.add(10).unwrap();
        let _node2 = list.add(20).unwrap();
        let _node3 = list.add(30).unwrap();
        assert_eq!(list.len(), 3);

        // Clear the list
        list.clear();
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());

        // Should be able to add new items
        let _node4 = list.add(40).unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_is_empty_and_is_full() {
        let mut list = List::<u32>::new(NonZeroUsize::new(2).unwrap());

        assert!(list.is_empty());
        assert!(!list.is_full());

        let _node1 = list.add(10).unwrap();
        assert!(!list.is_empty());
        assert!(!list.is_full());

        let _node2 = list.add(20).unwrap();
        assert!(!list.is_empty());
        assert!(list.is_full());

        list.remove_first();
        assert!(!list.is_empty());
        assert!(!list.is_full());

        list.remove_first();
        assert!(list.is_empty());
        assert!(!list.is_full());
    }

    struct ComplexValue {
        pub a: u32,
        pub b: String,
    }
    impl ComplexValue {
        fn new(a: u32, b: String) -> Self {
            ComplexValue { a, b }
        }
    }
    #[test]
    fn test_list_complex_values() {
        let mut list = List::<ComplexValue>::new(NonZeroUsize::new(2).unwrap());

        // Add complex values
        let node1 = list.add(ComplexValue::new(1, String::from("one"))).unwrap();
        let node2 = list.add(ComplexValue::new(2, String::from("two"))).unwrap();

        // Update complex value
        unsafe {
            let (old_val, success) =
                list.update(node1, ComplexValue::new(3, String::from("three")), true);
            let old_val = old_val.unwrap();
            assert_eq!(old_val.a, 1);
            assert_eq!(old_val.b, "one");
            assert!(success);
        }

        // Check updated value
        unsafe {
            let value = list.get_value(node1).unwrap();
            assert_eq!(value.a, 3);
            assert_eq!(value.b, "three");
        }

        // update locally
        unsafe {
            let value = list.get_value_mut(node2).unwrap();
            value.a = 4;
            value.b.push_str("_modified");
        }
        // Check updated value
        unsafe {
            let value = list.get_value(node2).unwrap();
            assert_eq!(value.a, 4);
            assert_eq!(value.b, "two_modified");
        }
    }

    // Additional tests to catch length management bugs
    #[test]
    fn test_attach_detach_length_management() {
        let mut list = List::<u32>::new(NonZeroUsize::new(3).unwrap());

        // Test that attach/attach_last don't increment length (for internal movement)
        let node = Box::into_raw(Box::new(Entry::new(10)));
        assert_eq!(list.len(), 0);

        unsafe {
            list.attach(node);
        }
        // attach should NOT increment length - it's for moving existing nodes
        assert_eq!(list.len(), 0, "attach should not increment length");

        // Clean up the first node manually since it's not tracked in length
        unsafe {
            list._detach(node);
            drop(Box::from_raw(node));
        }

        // Now test that attach_from_other_list DOES increment length
        let node2 = Box::into_raw(Box::new(Entry::new(20)));
        unsafe {
            list.attach_from_other_list(node2);
        }
        assert_eq!(
            list.len(),
            1,
            "attach_from_other_list should increment length"
        );

        // Clean up by removing the nodes - this will properly clean node2
        list.clear();
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_cross_list_node_transfer() {
        let mut list1 = List::<u32>::new(NonZeroUsize::new(3).unwrap());
        let mut list2 = List::<u32>::new(NonZeroUsize::new(3).unwrap());

        // Add items to list1
        let node1 = list1.add(10).unwrap();
        let _node2 = list1.add(20).unwrap();
        assert_eq!(list1.len(), 2);
        assert_eq!(list2.len(), 0);

        // Remove a node from list1
        let removed_node = unsafe { list1.remove(node1) }.unwrap();
        assert_eq!(list1.len(), 1);

        // Transfer to list2 using the cross-list method
        unsafe {
            list2.attach_from_other_list(Box::into_raw(removed_node));
        }
        assert_eq!(list1.len(), 1);
        assert_eq!(list2.len(), 1);

        // Verify both lists work correctly
        let from_list1 = list1.remove_first().unwrap();
        assert_eq!(unsafe { from_list1.val.assume_init() }, 20);

        let from_list2 = list2.remove_first().unwrap();
        assert_eq!(unsafe { from_list2.val.assume_init() }, 10);

        assert_eq!(list1.len(), 0);
        assert_eq!(list2.len(), 0);
    }

    #[test]
    fn test_attach_last_length_management() {
        let mut list = List::<u32>::new(NonZeroUsize::new(3).unwrap());

        // Test that attach_last doesn't increment length (for internal movement)
        let new_node = Box::into_raw(Box::new(Entry::new(20)));
        unsafe {
            list.attach_last(new_node);
        }
        // Length should still be 0 because attach_last doesn't increment length
        assert_eq!(list.len(), 0, "attach_last should not increment length");

        // Now test the cross-list version
        let new_node2 = Box::into_raw(Box::new(Entry::new(30)));
        unsafe {
            list.attach_last_from_other_list(new_node2);
        }
        assert_eq!(
            list.len(),
            1,
            "attach_last_from_other_list should increment length"
        );

        // Clean up
        list.clear();
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_move_to_front_length_invariant() {
        let mut list = List::<u32>::new(NonZeroUsize::new(3).unwrap());

        // Add items
        let node1 = list.add(10).unwrap();
        let node2 = list.add(20).unwrap();
        let node3 = list.add(30).unwrap();
        assert_eq!(list.len(), 3);

        // Move nodes around multiple times
        unsafe {
            list.move_to_front(node1); // Move 10 to front
        }
        assert_eq!(
            list.len(),
            3,
            "Length should remain constant after move_to_front"
        );

        unsafe {
            list.move_to_front(node2); // Move 20 to front
        }
        assert_eq!(
            list.len(),
            3,
            "Length should remain constant after move_to_front"
        );

        unsafe {
            list.move_to_front(node3); // Move 30 to front (already at front)
        }
        assert_eq!(
            list.len(),
            3,
            "Length should remain constant after move_to_front of head"
        );

        // Verify the list still works correctly
        list.clear();
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_add_unchecked_functionality() {
        let mut list = List::<u32>::new(NonZeroUsize::new(2).unwrap());

        // Fill the list normally
        let _node1 = list.add(10).unwrap();
        let _node2 = list.add(20).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.is_full());

        // Normal add should fail
        assert!(list.add(30).is_none());
        assert_eq!(list.len(), 2);

        // But add_unchecked should work
        let node3 = list.add_unchecked(30);
        assert_eq!(list.len(), 3);
        assert!(
            list.len() > list.cap().get(),
            "List should exceed capacity with add_unchecked"
        );

        // Verify the value was added correctly
        unsafe {
            let value = list.get_value(node3).unwrap();
            assert_eq!(*value, 30);
        }

        // List should still function correctly
        let first = list.remove_first().unwrap();
        assert_eq!(unsafe { first.val.assume_init() }, 30);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_length_consistency_after_complex_operations() {
        let mut list = List::<u32>::new(NonZeroUsize::new(4).unwrap());

        // Perform a series of complex operations and verify length consistency
        let node1 = list.add(10).unwrap();
        let node2 = list.add(20).unwrap();
        let node3 = list.add(30).unwrap();
        assert_eq!(list.len(), 3);

        // Move nodes around - this should NOT change length
        unsafe {
            list.move_to_front(node1);
        }
        assert_eq!(list.len(), 3, "Length unchanged after move_to_front");

        unsafe {
            list.move_to_front(node3);
        }
        assert_eq!(list.len(), 3, "Length unchanged after move_to_front");

        // Add one more item normally
        let node4 = list.add(40).unwrap();
        assert_eq!(list.len(), 4);
        assert!(list.is_full());

        // Now use add_unchecked to exceed capacity
        let _node5 = list.add_unchecked(50);
        assert_eq!(list.len(), 5);
        assert!(list.len() > list.cap().get(), "List should exceed capacity");

        // Remove nodes one by one and verify length decreases correctly
        let _r1 = list.remove_first().unwrap(); // Should remove node5 (50)
        assert_eq!(list.len(), 4);

        let _r2 = unsafe { list.remove(node2) }.unwrap(); // Remove node2 (20)
        assert_eq!(list.len(), 3);

        let _r3 = unsafe { list.remove(node4) }.unwrap(); // Remove node4 (40)
        assert_eq!(list.len(), 2);

        // Clear the rest
        list.clear();
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());

        // Should be able to add new items
        let _new_node = list.add(100).unwrap();
        assert_eq!(list.len(), 1);
    }
}
