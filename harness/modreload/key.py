"""Focuses the top-level window and presses one X key on $DISPLAY with XTest (no xdotool or window manager here): key.py Shift_R"""
import ctypes, sys, time
x = ctypes.CDLL("libX11.so.6"); t = ctypes.CDLL("libXtst.so.6")
V = ctypes.c_void_p
x.XOpenDisplay.restype = V
x.XStringToKeysym.restype = ctypes.c_ulong
x.XDefaultRootWindow.argtypes = [V]; x.XDefaultRootWindow.restype = ctypes.c_ulong
x.XKeysymToKeycode.argtypes = [V, ctypes.c_ulong]
x.XQueryTree.argtypes = [V, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong),
                         ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)), ctypes.POINTER(ctypes.c_uint)]
x.XSetInputFocus.argtypes = [V, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
x.XWarpPointer.argtypes = [V, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint, ctypes.c_int, ctypes.c_int]
d = x.XOpenDisplay(None)
root = x.XDefaultRootWindow(d)
r, p, kids, n = ctypes.c_ulong(), ctypes.c_ulong(), ctypes.POINTER(ctypes.c_ulong)(), ctypes.c_uint()
x.XQueryTree(d, root, ctypes.byref(r), ctypes.byref(p), ctypes.byref(kids), ctypes.byref(n))
x.XFetchName.argtypes = [V, ctypes.c_ulong, ctypes.POINTER(ctypes.c_char_p)]
for i in range(n.value):
    name = ctypes.c_char_p()
    if x.XFetchName(d, kids[i], ctypes.byref(name)) and name.value and b"Minecraft" in name.value:
        x.XSetInputFocus(d, kids[i], 1, 0)
x.XWarpPointer(d, 0, root, 0, 0, 0, 0, 400, 240)
k = x.XKeysymToKeycode(d, x.XStringToKeysym(sys.argv[1].encode()))
time.sleep(0.5)
for down in (1, 0):
    t.XTestFakeKeyEvent(V(d), k, down, 0); x.XFlush(V(d)); time.sleep(0.2)
