# Locomotion recordings for PR #5

Recorded from the real game (text renderer) with a fixed 16 ms timestep,
a held-key walk `LLLLL.......D......RRRRUU.......DDDD......` (each letter
holds that arrow key for 8 frames, `.` releases), cropped to the starting
room and scaled 3x.

- `locomotion-realtime.mp4`: 60 fps, real time.
- `locomotion-slow3x.mp4`, `locomotion-slow3x.gif`: the same frames at 1/3 speed.
- `locomotion-trace.csv`: the player's logical cell and drawn position (cells)
  and hop height for every recorded frame.

# Child animation before/after (PR for children following their parent)

Same recorder and fixed timestep, walk `RR...L...U...D...LLL..R......`,
recorded on `main` (left) and with children animated relative to their
parent (right). The right side was re-recorded on PR #8's latest code (children
animated by the shared object animation along `Path::Orbit`).

- `children-before-after-realtime.mp4`: 60 fps, real time.
- `children-before-after-slow3x.mp4`, `children-before-after-slow3x.gif`:
  the same at 1/3 speed.
