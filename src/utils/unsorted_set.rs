pub struct UnsortedSet<T, const N: usize> {
    data: [Option<T>; N],
    len: usize,
    overflow: Vec<T>,
}

impl<T, const N: usize> UnsortedSet<T, N>
where
    T: PartialEq,
{
    pub const fn new() -> Self {
        Self {
            data: [const { None }; N],
            len: 0,
            overflow: Vec::new(),
        }
    }

    // If the item isn't already in the set, add it.
    pub fn insert(&mut self, value: T) -> bool {
        if self.contains(&value) {
            return false;
        }

        // len is less than N so there is a free slot *somewhere*
        if self.len < N {
            for i in 0..N {
                if self.data[i].is_none() {
                    self.data[i] = Some(value);
                    self.len += 1;
                    return true;
                }
            }

            unreachable!();
        }

        self.overflow.push(value);
        true
    }

    pub fn remove(&mut self, value: &T) -> bool {
        for i in 0..self.len {
            if let Some(data) = &self.data[i] {
                if data == value {
                    self.data[i] = None;
                    self.len -= 1;

                    // If the array is not full and there are elements in the overflow
                    // then fill the gap with the last element.
                    if self.len < N && !self.overflow.is_empty() {
                        if let Some(data) = self.overflow.pop() {
                            self.data[i] = Some(data);
                            self.len += 1;
                        }
                    }

                    return true;
                }
            }
        }

        if let Some(index) = self.overflow.iter().position(|item| item == value) {
            self.overflow.swap_remove(index);
            return true;
        }

        false
    }

    pub fn contains(&mut self, value: &T) -> bool {
        for i in 0..N {
            if let Some(data) = &self.data[i] {
                if data == value {
                    return true;
                }
            }
        }

        self.overflow.contains(value)
    }

    pub fn len(&self) -> usize {
        self.len + self.overflow.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data
            .iter()
            .filter_map(|item| item.as_ref())
            .chain(self.overflow.iter())
    }
}
