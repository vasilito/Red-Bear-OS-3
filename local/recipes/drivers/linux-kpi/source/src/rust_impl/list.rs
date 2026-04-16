use std::ptr;

#[repr(C)]
pub struct ListHead {
    pub next: *mut ListHead,
    pub prev: *mut ListHead,
}

#[no_mangle]
pub extern "C" fn init_list_head(head: *mut ListHead) {
    if head.is_null() {
        return;
    }
    unsafe {
        (*head).next = head;
        (*head).prev = head;
    }
}

#[no_mangle]
pub extern "C" fn list_add(new: *mut ListHead, head: *mut ListHead) {
    if new.is_null() || head.is_null() {
        return;
    }

    unsafe {
        let next = (*head).next;
        (*new).next = next;
        (*new).prev = head;
        (*head).next = new;
        if !next.is_null() {
            (*next).prev = new;
        }
    }
}

#[no_mangle]
pub extern "C" fn list_add_tail(new: *mut ListHead, head: *mut ListHead) {
    if new.is_null() || head.is_null() {
        return;
    }

    unsafe {
        let prev = (*head).prev;
        (*new).next = head;
        (*new).prev = prev;
        (*head).prev = new;
        if !prev.is_null() {
            (*prev).next = new;
        }
    }
}

#[no_mangle]
pub extern "C" fn list_del(entry: *mut ListHead) {
    if entry.is_null() {
        return;
    }

    unsafe {
        let prev = (*entry).prev;
        let next = (*entry).next;
        if !prev.is_null() {
            (*prev).next = next;
        }
        if !next.is_null() {
            (*next).prev = prev;
        }
        (*entry).next = ptr::null_mut();
        (*entry).prev = ptr::null_mut();
    }
}

#[no_mangle]
pub extern "C" fn list_empty(head: *const ListHead) -> i32 {
    if head.is_null() {
        return 1;
    }
    if ptr::eq(unsafe { (*head).next } as *const ListHead, head) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn list_splice(list: *mut ListHead, head: *mut ListHead) {
    if list.is_null() || head.is_null() || list_empty(list) != 0 {
        return;
    }

    unsafe {
        let first = (*list).next;
        let last = (*list).prev;
        let at = (*head).next;

        (*first).prev = head;
        (*head).next = first;

        (*last).next = at;
        if !at.is_null() {
            (*at).prev = last;
        }
    }
}

#[no_mangle]
pub extern "C" fn list_first_entry(head: *const ListHead, offset: usize) -> *mut u8 {
    if head.is_null() || list_empty(head) != 0 {
        return ptr::null_mut();
    }

    let first = unsafe { (*head).next };
    if first.is_null() {
        return ptr::null_mut();
    }

    (first as usize)
        .checked_sub(offset)
        .map_or(ptr::null_mut(), |entry| entry as *mut u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    #[repr(C)]
    struct Node {
        value: u32,
        link: ListHead,
    }

    #[test]
    fn list_add_delete_and_first_entry_work() {
        let mut head = ListHead {
            next: ptr::null_mut(),
            prev: ptr::null_mut(),
        };
        init_list_head(&mut head);
        assert_eq!(list_empty(&head), 1);

        let mut node = Node {
            value: 7,
            link: ListHead {
                next: ptr::null_mut(),
                prev: ptr::null_mut(),
            },
        };
        list_add(&mut node.link, &mut head);
        assert_eq!(list_empty(&head), 0);

        let first = list_first_entry(&head, offset_of!(Node, link)).cast::<Node>();
        assert_eq!(unsafe { (*first).value }, 7);

        list_del(&mut node.link);
        init_list_head(&mut head);
        assert_eq!(list_empty(&head), 1);
    }

    #[test]
    fn list_add_tail_and_splice_work() {
        let mut dst = ListHead {
            next: ptr::null_mut(),
            prev: ptr::null_mut(),
        };
        let mut src = ListHead {
            next: ptr::null_mut(),
            prev: ptr::null_mut(),
        };
        init_list_head(&mut dst);
        init_list_head(&mut src);

        let mut node1 = Node {
            value: 1,
            link: ListHead {
                next: ptr::null_mut(),
                prev: ptr::null_mut(),
            },
        };
        let mut node2 = Node {
            value: 2,
            link: ListHead {
                next: ptr::null_mut(),
                prev: ptr::null_mut(),
            },
        };

        list_add_tail(&mut node1.link, &mut src);
        list_add_tail(&mut node2.link, &mut src);
        list_splice(&mut src, &mut dst);

        let first = list_first_entry(&dst, offset_of!(Node, link)).cast::<Node>();
        assert_eq!(unsafe { (*first).value }, 1);
        assert!(std::ptr::eq(node1.link.next, &mut node2.link));
    }
}
