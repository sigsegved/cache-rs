# Safety Considerations for the Cache Crate

This document outlines the safety considerations for users of the Cache crate, particularly when working with the `List` implementation.

## Raw Pointers and Unsafe Code

The `List` implementation uses raw pointers extensively to create an efficient doubly-linked list. This approach allows for O(1) operations but requires careful attention to safety.

### General Safety Guidelines

1. **Never free a node pointer manually** that has been returned by `add()` - the `List` owns the memory and will handle deallocation.

2. **Never use a pointer after the `List` is dropped** - all node pointers become invalid when the owning `List` is dropped.

3. **Never use a pointer after calling `remove_first()` or `remove_last()`** if that pointer might have been to the removed node.

4. **Node pointers are not thread-safe** - do not share node pointers across threads without proper synchronization.

### Unsafe Function Requirements

Functions marked `unsafe` in the API have specific requirements that must be met:

- `update(node, value, capture)`: The `node` must be a valid pointer to an `Entry` within the list.
- `get_value(node)` and `get_value_mut(node)`: The `node` must be a valid pointer to a non-sigil `Entry` within the list.
- `move_to_front(node)`: The `node` must be a valid pointer to an `Entry` within the list.
- `attach_last(node)`: The `node` must be a valid pointer to an `Entry` that is not already in the list.

### Common Pitfalls

1. **Using a node after it's been removed**: Always check if a node has been removed before using its pointer.

2. **Double insertion**: Never add a node that's already in the list. This will create cycles in the linked list.

3. **Null pointers**: While the API tries to handle null pointers gracefully, passing null pointers to unsafe functions is undefined behavior.

4. **Invalid memory**: Using pointers to freed memory will cause undefined behavior. The `List` implementation takes care of freeing memory when nodes are removed or when the list is dropped.

## Internal Safety Mechanisms

The `List` implementation uses several techniques to ensure memory safety:

1. **Sentinel nodes**: The list uses head and tail sentinel nodes to simplify operations and handle edge cases.

2. **Capacity limits**: The list enforces capacity limits to prevent unbounded growth.

3. **Null checks**: Functions check for null pointers where appropriate.

4. **Proper cleanup on drop**: The `Drop` implementation ensures all nodes are properly freed.

## Best Practices

1. **Use wrapper types**: Consider creating safe wrapper types around the raw pointer API if you need to use the `List` directly.

2. **Use higher-level cache implementations**: Where possible, use the higher-level cache implementations provided by this crate instead of working with the `List` directly.

3. **Document unsafe code**: When writing unsafe code that uses this `List` implementation, document the safety invariants you're maintaining.

4. **Test thoroughly**: Test unsafe code extensively, including edge cases like empty lists and full lists.
