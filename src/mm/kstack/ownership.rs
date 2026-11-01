//! Fixed-capacity ownership records. Retirement never releases a backing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    index: usize,
    generation: u64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State { Vacant, Reserved, Live, Retired }
struct Slot<T> { generation: u64, state: State, value: Option<T> }
impl<T> Slot<T> {
    const fn new() -> Self {
        Self { generation: 0, state: State::Vacant, value: None }
    }
}
pub struct Registry<T, const N: usize> { slots: [Slot<T>; N] }
impl<T, const N: usize> Registry<T, N> {
    pub const fn new() -> Self { Self { slots: [const { Slot::new() }; N] } }
    pub fn reserve(&mut self) -> Option<Token> {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.state != State::Vacant { continue; }
            let Some(generation) = slot.generation.checked_add(1) else { continue };
            slot.generation = generation;
            slot.state = State::Reserved;
            return Some(Token { index, generation });
        }
        None
    }
    fn slot_mut(&mut self, token: Token) -> Option<&mut Slot<T>> {
        self.slots.get_mut(token.index).filter(|s| s.generation == token.generation)
    }
    pub fn cancel(&mut self, token: Token) -> bool {
        let Some(slot) = self.slot_mut(token) else { return false };
        if slot.state != State::Reserved { return false; }
        slot.state = State::Vacant;
        true
    }
    pub fn install(&mut self, token: Token, value: T) -> Result<(), T> {
        let Some(slot) = self.slot_mut(token) else { return Err(value) };
        if slot.state != State::Reserved { return Err(value); }
        slot.value = Some(value);
        slot.state = State::Live;
        Ok(())
    }
    pub fn retire(&mut self, token: Token) -> bool {
        let Some(slot) = self.slot_mut(token) else { return false };
        if slot.state != State::Live { return false; }
        slot.state = State::Retired;
        true
    }
    pub fn retire_matching(&mut self, matches: impl Fn(&T) -> bool) -> bool {
        for slot in &mut self.slots {
            if slot.state == State::Live && slot.value.as_ref().is_some_and(&matches) {
                slot.state = State::Retired;
                return true;
            }
        }
        false
    }
    pub fn first_retired(&self) -> Option<(Token, &T)> {
        self.slots.iter().enumerate().find_map(|(index, slot)| {
            if slot.state != State::Retired { return None; }
            Some((Token { index, generation: slot.generation }, slot.value.as_ref()?))
        })
    }
    pub fn take_retired(&mut self, token: Token) -> Option<T> {
        let slot = self.slot_mut(token)?;
        if slot.state != State::Retired { return None; }
        let value = slot.value.take();
        slot.state = State::Vacant;
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_and_reserved_records_cannot_be_reclaimed() {
        let mut registry = Registry::<usize, 1>::new();
        let token = registry.reserve().unwrap();
        assert_eq!(registry.take_retired(token), None);
        registry.install(token, 42).unwrap();
        assert_eq!(registry.take_retired(token), None);
        assert!(registry.first_retired().is_none());
        assert!(registry.reserve().is_none());
    }
    #[test]
    fn retirement_retains_backing_until_explicit_take() {
        let mut registry = Registry::<usize, 1>::new();
        let token = registry.reserve().unwrap();
        registry.install(token, 42).unwrap();
        assert!(registry.retire(token));
        assert!(!registry.retire(token));
        assert_eq!(registry.first_retired().map(|(_, value)| *value), Some(42));
        assert!(registry.reserve().is_none());
        assert_eq!(registry.take_retired(token), Some(42));
        assert_eq!(registry.take_retired(token), None);
    }
    #[test]
    fn stale_tokens_do_not_retire_reused_slots() {
        let mut registry = Registry::<usize, 1>::new();
        let first = registry.reserve().unwrap();
        registry.install(first, 1).unwrap();
        registry.retire(first);
        assert_eq!(registry.take_retired(first), Some(1));
        let second = registry.reserve().unwrap();
        assert_ne!(first, second);
        registry.install(second, 2).unwrap();
        assert!(!registry.retire(first));
        assert_eq!(registry.take_retired(first), None);
        assert!(registry.retire(second));
        assert_eq!(registry.take_retired(second), Some(2));
    }
    #[test]
    fn cancelled_reservation_is_reusable_and_lookup_is_idempotent() {
        let mut registry = Registry::<usize, 1>::new();
        let first = registry.reserve().unwrap();
        assert!(registry.cancel(first));
        assert!(!registry.cancel(first));
        let next = registry.reserve().unwrap();
        registry.install(next, 123).unwrap();
        assert!(!registry.retire_matching(|value| *value == 999));
        assert!(registry.retire_matching(|value| *value == 123));
        assert!(!registry.retire_matching(|value| *value == 123));
        assert_eq!(registry.take_retired(next), Some(123));
    }
}
