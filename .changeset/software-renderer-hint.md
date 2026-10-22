---
default: minor
---

The launcher now says when the game is being drawn by the processor. A machine with no working graphics driver - a laptop whose driver was never installed, or a card the game fell back to software for - draws the whole game on the processor, and then no setting can make it fast: the frames are made by the same processor that runs the game. The mod can see this while the game runs, but the launcher can say it before the next launch: it reads the renderer out of the log the last launch left, and when that renderer is a software one the launch carries a toast that names it and says the machine needs a working graphics driver. On a laptop that driver usually comes from the maker of the machine.
