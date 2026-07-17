// Cascade scenario: two signals + two reactions (one unguarded, one guarded).
//
// Teaching goal: prove three things about cross-signal cascades in one
// walk-through — (1) a reaction fires a derived event on another signal
// (cascade trigger), (2) a guarded reaction is selectively skipped when its
// guard is false while its sibling would fire, and (3) the main transition
// commits regardless of whether any reaction fires or is skipped.
//
// reaction A (unguarded): when `task` enters `running`, nudge `ui` to `busy`.
// reaction B (guarded):   when `task` enters `done`, nudge `ui` to `done`
//                         — but only if the source payload says `record`.
signal task {
    states: [idle, running, done]
    initial: idle

    on start from idle -> running {
        on_transition: begin_work
    }

    on finish from running -> done {
        on_transition: complete_work
    }
}

signal ui {
    states: [ready, busy, done]
    initial: ready

    on show_busy from ready -> busy {
        on_transition: render_busy
    }

    on show_done from busy -> done {
        on_transition: render_done
    }
}

reaction {
    when task enters running -> ui show_busy
}

reaction {
    when task enters done -> ui show_done when payload.record == true
}
