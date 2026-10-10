#!/usr/bin/env python3
"""An owned GUI target: input must arrive through the actual X11 event stream."""
import json, os, pathlib, sys
import gi
gi.require_version('Gtk', '3.0')
gi.require_version('GdkX11', '3.0')
from gi.repository import Gtk, GdkX11, GLib
output = pathlib.Path(sys.argv[1])
nonce = sys.argv[2]
window = Gtk.Window(title='Agent Research Lab Target ' + nonce)
window.set_default_size(640, 320)
window.set_position(Gtk.WindowPosition.CENTER)
box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=20)
box.set_border_width(30)
window.add(box)
box.pack_start(Gtk.Label(label='Dedicated computer-use research target'), False, False, 0)
entry = Gtk.Entry()
entry.set_placeholder_text('Type the verification token here')
box.pack_start(entry, False, False, 0)
label = Gtk.Label(label='Observed text: ')
box.pack_start(label, False, False, 0)
sequence = 0
key_events = 0

def publish():
    global sequence
    sequence += 1
    record = {'nonce': nonce, 'pid': os.getpid(), 'window_id': window.get_window().get_xid(),
              'title': window.get_title(), 'text': entry.get_text(),
              'sequence': sequence, 'key_events': key_events}
    temporary = output / 'observed.pending.json'
    temporary.write_text(json.dumps(record))
    os.replace(temporary, output / 'observed.json')

def changed(widget):
    label.set_text('Observed text: ' + widget.get_text())
    publish()

def pressed(widget, event):
    global key_events
    key_events += 1
    return False

entry.connect('key-press-event', pressed)
entry.connect('changed', changed)
window.connect('destroy', Gtk.main_quit)
window.show_all()
entry.grab_focus()
GLib.timeout_add(200, lambda: (publish(), False)[1])
Gtk.main()
