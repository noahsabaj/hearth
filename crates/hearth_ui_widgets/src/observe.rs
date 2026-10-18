// TODO: This probably doesn't belong in hearth_ui_widgets, but I am not sure where it should go.
// It is certainly a useful thing to have.
#![expect(unsafe_code, reason = "Unsafe code is used to improve performance.")]

use core::{marker::PhantomData, mem};

use hearth_ecs::{
    bundle::{Bundle, DynamicBundle},
    event::{EntityEvent, EventPattern},
    system::IntoObserverSystem,
};

/// Helper struct that adds an observer when inserted as a [`Bundle`].
pub struct AddObserver<E, M, I>
where
    E: EventPattern<Event: EntityEvent>,
    I: IntoObserverSystem<E, M>,
{
    observer: I,
    marker: PhantomData<(E, M)>,
}

// SAFETY: Empty method bodies.
unsafe impl<E, M, I> Bundle for AddObserver<E, M, I>
where
    E: EventPattern<Event: EntityEvent>,
    M: Send + Sync + 'static,
    I: IntoObserverSystem<E, M> + Send + Sync,
{
    #[inline]
    fn component_ids(
        _components: &mut hearth_ecs::component::ComponentsRegistrator,
    ) -> impl Iterator<Item = hearth_ecs::component::ComponentId> + use<E, M, I> {
        // SAFETY: Empty iterator
        core::iter::empty()
    }

    #[inline]
    fn get_component_ids(
        _components: &hearth_ecs::component::Components,
    ) -> impl Iterator<Item = Option<hearth_ecs::component::ComponentId>> {
        // SAFETY: Empty iterator
        core::iter::empty()
    }
}

impl<E, M, I> DynamicBundle for AddObserver<E, M, I>
where
    E: EventPattern<Event: EntityEvent>,
    I: IntoObserverSystem<E, M>,
{
    type Effect = Self;

    #[inline]
    unsafe fn get_components(
        ptr: hearth_ecs::ptr::MovingPtr<'_, Self>,
        _func: &mut impl FnMut(hearth_ecs::component::StorageType, hearth_ecs::ptr::OwningPtr<'_>),
    ) {
        // SAFETY: We must not drop the pointer here, or it will be uninitialized in `apply_effect`
        // below.
        mem::forget(ptr);
    }

    #[inline]
    unsafe fn apply_effect(
        ptr: hearth_ecs::ptr::MovingPtr<'_, mem::MaybeUninit<Self>>,
        entity: &mut hearth_ecs::world::EntityWorldMut,
    ) {
        // SAFETY: The pointer was not dropped in `get_components`, so the allocation is still
        // initialized.
        let add_observer = unsafe { ptr.assume_init() };
        if entity.is_despawned() {
            return;
        }

        let add_observer = add_observer.read();
        entity.observe(add_observer.observer);
    }
}

/// Adds an observer as a bundle effect.
pub fn observe<E, M, I>(observer: I) -> AddObserver<E, M, I>
where
    E: EventPattern<Event: EntityEvent>,
    I: IntoObserverSystem<E, M>,
{
    AddObserver {
        observer,
        marker: PhantomData,
    }
}
