# Today timer modes

The selector above the timer offers **Normal** (the default) and **Pomodoro**.

## Normal

The counter advances each second while native tracking runs. Native monotonic time
is persisted every 15 seconds and when starting, pausing, stopping or quitting.
The renderer interpolates the latest native sample locally for a smooth one-second
counter. Renderer reloads and new analysis results do not add time twice. Paused
and stopped time is excluded, and local midnight starts the next day's total.

The Today goal rail and Insights daily total use this same tracking clock.
Activity categories and sustained focus remain based on analyzed observations;
the focus share explicitly states its analyzed denominator.

## Pomodoro

The countdown defaults to 25 minutes of work, 5 minutes of short break and 15
minutes of long break after every fourth completed work interval. The three
intervals are configurable while a session is paused. Mode and interval preferences
are saved locally. Switching modes preserves the day's tracking total.

Completing work pauses native tracking before starting the break countdown.
A local in-app message announces each transition. The next work interval waits
for the user to press Start; it never silently re-enables monitoring. End break
makes the next interval available without starting tracking. Stopping resets the
Pomodoro cycle. A manual work pause freezes the remaining countdown.

The feature is shared by Windows, macOS and Linux and works in free mode.
It does not create calendar events or send messaging replies.
