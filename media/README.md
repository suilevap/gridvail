# Locomotion recordings for PR #5

Recorded from the real game (text renderer) with a fixed 16 ms timestep,
a held-key walk `LLLLL.......D......RRRRUU.......DDDD......` (each letter
holds that arrow key for 8 frames, `.` releases), cropped to the starting
room and scaled 3x.

- `locomotion-realtime.mp4`: 60 fps, real time.
- `locomotion-slow3x.mp4`, `locomotion-slow3x.gif`: the same frames at 1/3 speed.
- `locomotion-trace.csv`: the player's logical cell and drawn position (cells)
  and hop height for every recorded frame.
