# Nyx Dock's Hyprland plugin. Built against the Hyprland on this machine (its headers
# come with the hyprland package); rebuild after Hyprland updates.

CXXFLAGS ?= -O2
PKGS = hyprland pixman-1 libdrm libinput libudev wayland-server xkbcommon

all: nyx-dock-swipe.so

nyx-dock-swipe.so: main.cpp
	$(CXX) -shared -fPIC --no-gnu-unique -std=c++2c $(CXXFLAGS) main.cpp -o $@ `pkg-config --cflags $(PKGS)`

clean:
	rm -f nyx-dock-swipe.so

.PHONY: all clean
