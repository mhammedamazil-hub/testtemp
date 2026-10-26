#!/usr/bin/env bash
# Collect hardware/OS evidence for feasibility documents.
echo "== CPU =="
lscpu | grep -E "Model name|^CPU\(s\)|Architecture" | head -4
echo "== RAM =="
free -h | head -2
echo "== DISK =="
df -h / | tail -1
echo "== GPU devices =="
ls -la /dev/dri 2>&1 | head -3
ls /dev/nvidia* 2>&1 | head -3
echo "== GL libraries (system) =="
ls /usr/lib/x86_64-linux-gnu/libGL* 2>&1 | head -3
echo "== OS / toolchain =="
. /etc/os-release
echo "$PRETTY_NAME"
ldd --version | head -1
gcc --version | head -1
echo -n "clang: "; which clang || echo absent
echo -n "cmake: "; which cmake || echo absent
echo "== virtualization hints =="
systemd-detect-virt 2>/dev/null || echo "systemd-detect-virt unavailable"
grep -m1 hypervisor /proc/cpuinfo || echo "no hypervisor flag (may still be a container)"
