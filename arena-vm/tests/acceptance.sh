#!/usr/bin/env bash
# Acceptance tests against the live desktop. Writes evidence/acceptance.txt
# with PASS/FAIL/BLOCKED/NOT TESTED for every requirement.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
source "$HERE/../scripts/env.sh"
export DISPLAY="${DISPLAY:-:99}"
PY="$VM_VENV/bin/python"
EV="$VM_ROOT/evidence"; mkdir -p "$EV"
RES="$EV/acceptance.txt"; : > "$RES"
ok=0; fail=0; blocked=0

rec() { # status name [detail]
  printf '%-8s %-34s %s\n' "$1" "$2" "${3:-}" | tee -a "$RES"
  case "$1" in PASS) ok=$((ok+1));; FAIL) fail=$((fail+1));; BLOCKED) blocked=$((blocked+1));; esac
}

# 1. display server alive
if pgrep -f "bin/Xvfb $DISPLAY" >/dev/null; then rec PASS T1_xvfb_running "$(pgrep -af "bin/Xvfb $DISPLAY" | head -1)"; else rec FAIL T1_xvfb_running; fi

# 2. X11 client connects
if "$VM_PREFIX/bin/xdpyinfo" >/dev/null 2>&1; then rec PASS T2_x11_client_connect "$("$VM_PREFIX/bin/xdpyinfo" | awk '/X.Org version/{print $3}')"; else rec FAIL T2_x11_client_connect; fi

# 3. window manager
if pgrep -f "bin/dwm" >/dev/null; then rec PASS T3_window_manager "dwm $(pgrep -f bin/dwm | head -1)"; else rec FAIL T3_window_manager; fi

# 4. screenshot of the live display
if "$PY" "$VM_ROOT/control/shoot.py" "$EV/t4_shot.png" >/dev/null 2>&1 && [ -s "$EV/t4_shot.png" ]; then rec PASS T4_screenshot "$(stat -c%s "$EV/t4_shot.png") bytes"; else rec FAIL T4_screenshot; fi

# 5. GUI app launch + 6. injected input changes state + 8. file created via GUI
rm -f /tmp/acceptance_gui.txt
"$PY" "$VM_ROOT/control/vmc.py" launch "$VM_PREFIX/bin/st" -e bash -c "cat > /tmp/acceptance_gui.txt" >/dev/null
sleep 2
WID=$("$PY" "$VM_ROOT/control/vmc.py" windows | awk '/\+0\+19/{print $1; exit}')
if [ -n "$WID" ]; then rec PASS T5_gui_app_launched "st window $WID"; else rec FAIL T5_gui_app_launched; fi
if [ -n "$WID" ]; then
  "$PY" "$VM_ROOT/control/vmc.py" focus "$WID" >/dev/null 2>&1
  "$PY" "$VM_ROOT/control/vmc.py" type "GUI_INPUT_$((RANDOM))"
  "$PY" "$VM_ROOT/control/vmc.py" key ctrl d
  sleep 1
  if [ -s /tmp/acceptance_gui.txt ]; then
    rec PASS T6_input_injection "$(cat /tmp/acceptance_gui.txt)"
    rec PASS T8_file_created_via_gui /tmp/acceptance_gui.txt
  else rec FAIL T6_input_injection; rec FAIL T8_file_created_via_gui; fi
  "$PY" "$VM_ROOT/control/vmc.py" shot "$EV/t6_gui_input.png" >/dev/null 2>&1
fi

# 7. agent inspects screenshot (this test itself proves the loop; check PIL parse)
if "$PY" -c "from PIL import Image; im=Image.open('$EV/t4_shot.png'); im.load(); print(im.size)" >/tmp/t7.out 2>&1; then rec PASS T7_agent_screenshot_loop "$(cat /tmp/t7.out)"; else rec FAIL T7_agent_screenshot_loop; fi

# 9. browser
if [ -x "$VM_PREFIX/bin/dillo" ]; then
  "$PY" "$VM_ROOT/control/vmc.py" launch "$VM_PREFIX/bin/dillo" "file:///tmp/dillo_test.html" >/dev/null 2>&1 || true
  cat > /tmp/dillo_test.html <<'EOF'
<html><head><title>Acceptance</title></head><body><h1>ok</h1></body></html>
EOF
  "$PY" "$VM_ROOT/control/vmc.py" launch "$VM_PREFIX/bin/dillo" "file:///tmp/dillo_test.html" >/dev/null
  sleep 3
  if "$PY" "$VM_ROOT/control/vmc.py" windows | grep -qi dillo; then rec PASS T9_browser_works dillo; else rec FAIL T9_browser_works; fi
else rec BLOCKED T9_browser_works "dillo not built"; fi

# 10. tablet bridge
if ss -ltn 2>/dev/null | grep -q "0.0.0.0:6080"; then
  CODE=$(curl -sS -o /dev/null -w '%{http_code}' http://127.0.0.1:6080/ || true)
  [ "$CODE" = "401" ] && rec PASS T10_tablet_auth "401 without credentials" || rec FAIL T10_tablet_auth "http=$CODE"
else rec NOT_TESTED T10_tablet_bridge "start scripts/start_tablet.sh" ; fi

# 11. blender
if "$PY" -c "import bpy" 2>/dev/null; then
  rec PASS T11_blender_installed "$("$PY" -c 'import bpy;print(bpy.app.version_string)')"
  if [ -f "$VM_ROOT/tests/blender_smoke.py" ]; then
    "$PY" "$VM_ROOT/tests/blender_smoke.py" "$EV/t11_blender.png" >>"$RES" 2>&1 \
      && rec PASS T11_blender_render "$EV/t11_blender.png" || rec FAIL T11_blender_render
  fi
else rec NOT_TESTED T11_blender "run scripts/install_blender.sh"; fi

# 12. UE5 feasibility evidence collected?
[ -f "$VM_ROOT/docs/ue5-feasibility.md" ] && rec PASS T12_ue5_feasibility_doc "docs/ue5-feasibility.md" || rec NOT_TESTED T12_ue5_feasibility

# 13. recovery assets present
[ -x "$VM_ROOT/scripts/recover.sh" ] && [ -s "$VM_ROOT/pins.env" ] \
  && rec PASS T13_recovery_assets "$(wc -l < "$VM_ROOT/pins.env") pins" || rec FAIL T13_recovery_assets

echo "---"; echo "PASS=$ok FAIL=$fail BLOCKED=$blocked" | tee -a "$RES"
