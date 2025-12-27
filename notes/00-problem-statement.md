# Problem Statement

Build a local, Linux-based system that records a user’s high-level computer activity as immutable, append-only events (e.g. application focus changes, shell commands, file interaction metadata) and allows the user to reconstruct and analyze past activity by replaying those events.

The system must treat event history as the single source of truth, derive all state (such as time spent per app, project, or language) from replay, and remain crash-safe, deterministic, and fully offline.

It must prioritize correctness, privacy, and explainability over features, UI polish, or scale.

## List Of things I want to track
- Track time spend on each software or terminal.
- - It's sum must be equal to the uptime of the system. As at any point of time, there is focus somewhere, even on the desktop screen.
- In software details iff possible.
- - Like I should not just know how much time I spent on terminal, but also the commands I ran or the files that I opened
- (Optional) Number of keystrokes I made ( Just feeling like would be fun to plot that on graph later)

These are the things I feel I should code for right now. Will add more later when any new idea arrives

## To start with

I would like to choose Window focus to start with. I would want to know how much time I have spent on which focused window.