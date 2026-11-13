"""Keep the pointer outside native QA windows on the verifier's private Xvfb display."""
import ctypes
import os


def park_pointer_until(stop):
    if os.environ.get('DISPLAY', '').split('.')[0] in (':0', ''):
        raise RuntimeError('Native GUI verifier requires its own Xvfb display')
    x11 = ctypes.CDLL('libX11.so.6')
    pointer, window = ctypes.c_void_p, ctypes.c_ulong
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]; x11.XOpenDisplay.restype = pointer
    x11.XDefaultRootWindow.argtypes = [pointer]; x11.XDefaultRootWindow.restype = window
    x11.XDefaultScreen.argtypes = [pointer]
    x11.XDisplayWidth.argtypes = [pointer, ctypes.c_int]
    x11.XDisplayHeight.argtypes = [pointer, ctypes.c_int]
    x11.XWarpPointer.argtypes = [pointer, window, window, ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint, ctypes.c_int, ctypes.c_int]
    x11.XSync.argtypes = [pointer, ctypes.c_int]; x11.XCloseDisplay.argtypes = [pointer]
    display = x11.XOpenDisplay(None)
    if not display: raise RuntimeError('No X11 test display')
    try:
        root, screen = x11.XDefaultRootWindow(display), x11.XDefaultScreen(display)
        width, height = x11.XDisplayWidth(display, screen), x11.XDisplayHeight(display, screen)
        while not stop.wait(.05):
            # New windows can cache an initial hover. An enter/leave after mapping clears it.
            for x, y in ((width//2, height//2), (width-10, height-10)):
                x11.XWarpPointer(display, 0, root, 0, 0, 0, 0, x, y)
                x11.XSync(display, 0)
    finally:
        x11.XCloseDisplay(display)
