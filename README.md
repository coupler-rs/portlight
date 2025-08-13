# Portlight

Portlight is a cross-platform window management library built for use in both standalone applications and embedded plugin GUIs. It currently supports Windows, macOS, and X11.

Supported functionality:

- Opening top-level and child windows
- Handling mouse input
- Setting the cursor icon
- Spawning timers
- Querying per-window scale factor (DPI) information
- Receiving monitor refresh (vsync) events
- Presenting a buffer of pixels to the screen

Not implemented yet:

- Keyboard input
- Window resizing
- Clipboard handling
- Drag and drop
- Opening a file dialog

No direct support is provided for using graphics APIs like Direct3D, Metal, OpenGL, or Vulkan, but it should be possible to do so manually using the `RawWindow` API.

## License

This project is distributed under the terms of both the [MIT license](LICENSE-MIT) and the [Apache license, version 2.0](LICENSE-APACHE). Contributions are accepted under the same terms.
