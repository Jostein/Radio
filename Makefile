APP_NAME := Radio
APP := build/$(APP_NAME).app

.PHONY: app run clean vm shot help

app:
	swift build -c release --build-system native
	rm -rf $(APP)
	mkdir -p $(APP)/Contents/MacOS $(APP)/Contents/Resources
	cp .build/release/$(APP_NAME) $(APP)/Contents/MacOS/$(APP_NAME)
	cp Resources/Info.plist $(APP)/Contents/Info.plist
	cp Resources/AppIcon.icns $(APP)/Contents/Resources/AppIcon.icns
	codesign --force --sign - $(APP)
	@echo "Built $(APP)"

run: app
	open $(APP)

clean:
	rm -rf .build build

# --- Windows prototype, on AppVeyor's machine -------------------------------
# Each target is one command you can retype; no npm, despite the shape.

# Open a session on the Windows build machine (starts or reuses a build).
vm:
	scripts/test-env.sh

# Show the machine's screen as the app left it (newest screenshot artifact).
shot:
	scripts/vm-shot.sh
	@echo "See .rdp/vm-live.jpg"

# Build and test the Windows prototype here, as far as this Mac can.
proto:
	cd Platforms && cargo test -p radio-core -p radio

help:
	@echo "make app    build the macOS app into build/Radio.app"
	@echo "make run    build and open it"
	@echo "make vm     open a session on the Windows build machine"
	@echo "make shot   save the machine's latest screen to .rdp/vm-live.jpg"
	@echo "make proto  test the Windows prototype crates"
