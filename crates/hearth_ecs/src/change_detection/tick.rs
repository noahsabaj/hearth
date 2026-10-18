use core::{
    cell::UnsafeCell,
    fmt::{self, Debug, Formatter},
    panic::Location,
    sync::atomic::{AtomicU32, Ordering},
};
use hearth_ecs_macros::Event;
#[cfg(feature = "hearth_reflect")]
use hearth_reflect::Reflect;

use crate::change_detection::{MaybeLocation, MAX_CHANGE_AGE};

/// A value that tracks when a system ran relative to other systems.
/// This is used to power change detection.
///
/// *Note* that a system that hasn't been run yet has a `Tick` of 0.
#[derive(Copy, Clone, Default, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(
    feature = "hearth_reflect",
    derive(Reflect),
    reflect(Debug, Hash, PartialEq, Clone)
)]
pub struct Tick {
    tick: u32,
}

impl Tick {
    /// The maximum relative age for a change tick.
    /// The value of this is equal to [`MAX_CHANGE_AGE`].
    ///
    /// Since change detection will not work for any ticks older than this,
    /// ticks are periodically scanned to ensure their relative values are below this.
    pub const MAX: Self = Self::new(MAX_CHANGE_AGE);

    /// Creates a new [`Tick`] wrapping the given value.
    #[inline]
    pub const fn new(tick: u32) -> Self {
        Self { tick }
    }

    /// Gets the value of this change tick.
    #[inline]
    pub const fn get(self) -> u32 {
        self.tick
    }

    /// Sets the value of this change tick.
    #[inline]
    pub fn set(&mut self, tick: u32) {
        self.tick = tick;
    }

    /// Returns `true` if this `Tick` occurred since the system's `last_run`.
    ///
    /// `this_run` is the current tick of the system, used as a reference to help deal with wraparound.
    #[inline]
    pub fn is_newer_than(self, last_run: Tick, this_run: Tick) -> bool {
        // This works even with wraparound because the world tick (`this_run`) is always "newer" than
        // `last_run` and `self.tick`, and we scan periodically to clamp `ComponentTicks` values
        // so they never get older than `u32::MAX` (the difference would overflow).
        //
        // The clamp here ensures determinism (since scans could differ between app runs).
        let ticks_since_insert = this_run.relative_to(self).tick.min(MAX_CHANGE_AGE);
        let ticks_since_system = this_run.relative_to(last_run).tick.min(MAX_CHANGE_AGE);

        ticks_since_system > ticks_since_insert
    }

    /// Returns a change tick representing the relationship between `self` and `other`.
    #[inline]
    pub(crate) fn relative_to(self, other: Self) -> Self {
        let tick = self.tick.wrapping_sub(other.tick);
        Self { tick }
    }

    /// Wraps this change tick's value if it exceeds [`Tick::MAX`].
    ///
    /// Returns `true` if wrapping was performed. Otherwise, returns `false`.
    #[inline]
    pub fn check_tick(&mut self, check: CheckChangeTicks) -> bool {
        let age = check.present_tick().relative_to(*self);
        // This comparison assumes that `age` has not overflowed `u32::MAX` before, which will be true
        // so long as this check always runs before that can happen.
        if age.get() > Self::MAX.get() {
            *self = check.present_tick().relative_to(Self::MAX);
            true
        } else {
            false
        }
    }
}

/// A tick that can be updated from multiple threads.
///
/// This has exactly the same semantics as [`Tick`] but can be updated
/// atomically. It's used for summary ticks.
#[derive(Default)]
pub struct AtomicTick {
    /// The atomic tick value.
    tick: AtomicU32,
}

impl AtomicTick {
    /// Returns the current value of the tick.
    pub fn get(&self) -> Tick {
        Tick {
            tick: self.tick.load(Ordering::Relaxed),
        }
    }

    #[expect(
        clippy::doc_markdown,
        reason = "The word 'ARMv6' does not require backticks"
    )]
    /// Sets a new value for the tick.
    ///
    /// Note that this method takes `&self` and can therefore be called from
    /// multiple threads. The tick is updated using relaxed ordering and is
    /// therefore cheap to update on common architectures (x86-64, ARMv6 and
    /// newer). However, be warned that, because it uses relaxed ordering, a
    /// full mutex lock or similar barrier is required if you need to coordinate
    /// the synchronization of this value with other memory locations.
    pub fn set(&self, new_tick: Tick) {
        // Do an unsynchronized read first.
        //
        // This is important on x86-64 (both Intel and AMD) to avoid a
        // performance cliff. It has much smaller effects on AArch64, but it's
        // still worth doing on e.g. Apple M2 in parallel mode, so I'm leaving
        // it in.
        //
        // See benchmarks:
        // upstream PR #25157
        if self.tick.load(Ordering::Relaxed) != new_tick.get() {
            self.tick.store(new_tick.get(), Ordering::Relaxed);
        }
    }
}

impl Debug for AtomicTick {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("AtomicTick")
            .field("tick", &self.get().get())
            .finish()
    }
}

/// The summary ticks a change to some rows of a table column must update: the tick of the whole
/// column, and the ticks of the chunks of [`SummaryTicks::CHUNK_ROWS`] rows those rows lie in.
///
/// Queries filtered by [`Changed`](crate::query::Changed) or [`Added`](crate::query::Added) read
/// the column's tick to skip a table and each chunk's tick to skip its rows, so a few changes in a
/// large table don't make every query look at every row.
///
/// Updating more chunks than the changed rows lie in is always allowed: it only makes queries look
/// at rows they could have skipped.
#[derive(Copy, Clone, Debug)]
pub struct SummaryTicks<'a> {
    column: &'a AtomicTick,
    chunks: &'a [AtomicTick],
}

impl<'a> SummaryTicks<'a> {
    /// How many consecutive rows of a column share a chunk tick.
    pub const CHUNK_ROWS: usize = 256;

    /// Creates summary ticks from the column's tick and the chunk ticks to update along with it.
    pub fn new(column: &'a AtomicTick, chunks: &'a [AtomicTick]) -> Self {
        Self { column, chunks }
    }

    /// Returns the chunk that `row` lies in.
    #[inline]
    pub const fn chunk_of(row: usize) -> usize {
        row / Self::CHUNK_ROWS
    }

    /// Returns the column's summary tick: the last time any row of the column changed.
    #[inline]
    pub fn get(&self) -> Tick {
        self.column.get()
    }

    /// Returns the chunk ticks these summary ticks update.
    #[inline]
    pub fn chunks(&self) -> &'a [AtomicTick] {
        self.chunks
    }

    /// Records a change at `tick` to the column and to every chunk.
    #[inline]
    pub fn set(&self, tick: Tick) {
        for chunk in self.chunks {
            chunk.set(tick);
        }
        self.column.set(tick);
    }

    /// Records, at `this_run`, a value whose change tick was set to `changed`, wherever `changed`
    /// is newer than the summary.
    #[inline]
    pub fn include(&self, changed: Tick, this_run: Tick) {
        for summary in self.chunks.iter().chain(core::iter::once(self.column)) {
            if changed.is_newer_than(summary.get(), this_run) {
                summary.set(this_run);
            }
        }
    }

    /// Narrows the chunks to the one that `row` lies in. `self` must cover the whole column.
    #[inline]
    pub fn row(self, row: usize) -> Self {
        let chunk = Self::chunk_of(row);
        Self {
            column: self.column,
            chunks: &self.chunks[chunk..=chunk],
        }
    }

    /// Like [`Self::row`], without checking that the chunk exists, so that narrowing compiles to
    /// nothing when the result goes unused, as it does for every value a query fetches mutably
    /// but never changes.
    ///
    /// # Safety
    ///
    /// `self` must cover the whole column, and `row` must be less than the column's capacity.
    #[inline(always)]
    pub(crate) unsafe fn row_unchecked(self, row: usize) -> Self {
        let chunk = Self::chunk_of(row);
        Self {
            column: self.column,
            // SAFETY: The caller ensures the column, and so its chunk ticks, covers `row`.
            chunks: core::slice::from_ref(unsafe { self.chunks.get_unchecked(chunk) }),
        }
    }

    /// Narrows the chunks to the ones that `rows` lie in. `self` must cover the whole column.
    #[inline]
    pub fn rows(self, rows: core::ops::Range<usize>) -> Self {
        if rows.is_empty() {
            return Self {
                column: self.column,
                chunks: &[],
            };
        }
        Self {
            column: self.column,
            chunks: &self.chunks[Self::chunk_of(rows.start)..=Self::chunk_of(rows.end - 1)],
        }
    }
}

/// An [`Event`] that can be used to maintain [`Tick`]s in custom data structures, enabling to make
/// use of hearth's periodic checks that clamps ticks to a certain range, preventing overflows and thus
/// keeping methods like [`Tick::is_newer_than`] reliably return `false` for ticks that got too old.
///
/// # Example
///
/// Here a schedule is stored in a custom resource. This way the systems in it would not have their change
/// ticks automatically updated via [`World::check_change_ticks`](crate::world::World::check_change_ticks),
/// possibly causing `Tick`-related bugs on long-running apps.
///
/// To fix that, add an observer for this event that calls the schedule's
/// [`Schedule::check_change_ticks`](hearth_ecs::schedule::Schedule::check_change_ticks).
///
/// ```
/// use hearth_ecs::prelude::*;
/// use hearth_ecs::change_detection::CheckChangeTicks;
///
/// #[derive(Resource)]
/// struct CustomSchedule(Schedule);
///
/// # let mut world = World::new();
/// world.add_observer(|check: On<CheckChangeTicks>, mut schedule: ResMut<CustomSchedule>| {
///     schedule.0.check_change_ticks(*check);
/// });
/// ```
#[derive(Debug, Clone, Copy, Event)]
pub struct CheckChangeTicks(pub(crate) Tick);

impl CheckChangeTicks {
    /// Get the present `Tick` that other ticks get compared to.
    pub fn present_tick(self) -> Tick {
        self.0
    }
}

/// Interior-mutable access to the [`Tick`]s of a single component or resource.
#[derive(Copy, Clone, Debug)]
pub struct ComponentTickCells<'a> {
    /// The tick indicating when the value was added to the world.
    pub added: &'a UnsafeCell<Tick>,
    /// The tick indicating the last time the value was modified.
    pub changed: &'a UnsafeCell<Tick>,
    /// The calling location that last modified the value.
    pub changed_by: MaybeLocation<&'a UnsafeCell<&'static Location<'static>>>,
    /// The summary ticks for the value's column and chunk, if the component is dense and has a
    /// summary tick.
    pub summary_tick: Option<SummaryTicks<'a>>,
}

/// Records when a component or resource was added and when it was last mutably dereferenced (or added).
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "hearth_reflect", derive(Reflect), reflect(Debug, Clone))]
pub struct ComponentTicks {
    /// Tick recording the time this component or resource was added.
    pub added: Tick,

    /// Tick recording the time this component or resource was most recently changed.
    pub changed: Tick,
}

impl ComponentTicks {
    /// Returns `true` if the component or resource was added after the system last ran
    /// (or the system is running for the first time).
    #[inline]
    pub fn is_added(&self, last_run: Tick, this_run: Tick) -> bool {
        self.added.is_newer_than(last_run, this_run)
    }

    /// Returns `true` if the component or resource was added or mutably dereferenced after the system last ran
    /// (or the system is running for the first time).
    #[inline]
    pub fn is_changed(&self, last_run: Tick, this_run: Tick) -> bool {
        self.changed.is_newer_than(last_run, this_run)
    }

    /// Creates a new instance with the same change tick for `added` and `changed`.
    pub fn new(change_tick: Tick) -> Self {
        Self {
            added: change_tick,
            changed: change_tick,
        }
    }

    /// Manually sets the change tick.
    ///
    /// This is normally done automatically via the [`DerefMut`](core::ops::DerefMut) implementation
    /// on [`Mut<T>`](crate::change_detection::Mut), [`ResMut<T>`](crate::change_detection::ResMut), etc.
    /// However, components and resources that make use of interior mutability might require manual updates.
    ///
    /// # Example
    /// ```no_run
    /// # use hearth_ecs::{world::World, change_detection::ComponentTicks};
    /// let world: World = unimplemented!();
    /// let component_ticks: ComponentTicks = unimplemented!();
    ///
    /// component_ticks.set_changed(world.read_change_tick());
    /// ```
    #[inline]
    pub fn set_changed(&mut self, change_tick: Tick) {
        self.changed = change_tick;
    }
}
