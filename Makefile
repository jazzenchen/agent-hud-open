APP := build/Agent HUD Open.app
BIN := $(APP)/Contents/MacOS/Agent HUD Open
SNAPSHOT_DIR ?= build/snapshots

.PHONY: build test check run demo snapshot clean

build:
	@scripts/build-app.sh debug

check:
	python3 scripts/check-source-boundaries.py

test:
	swift test

run: build
	open "$(APP)"

demo: build
	open "$(APP)" --args --demo --show-settings

snapshot: build
	@"$(BIN)" --snapshot "$(SNAPSHOT_DIR)"

clean:
	rm -rf .build build
