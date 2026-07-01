# Self-contained offset-install workload for fakeroot-style benchmarking.
#
# Phase 1 (`all`): compile N small programs and static libs under build/.
# Phase 2 (`package`): stage under $(DESTDIR)$(prefix)/…, create device nodes,
# then archive the tree with tar (stats every entry — typical source package flow).
#
# Ownership mix (exercises map entries for both root and installer uid):
#   bin/, sbin/, dev/  → root (0:0)
#   lib/, share/       → installer ($(INSTALLER_UID):$(INSTALLER_GID))
#
# Usage (from a generated workdir):
#   make -f /path/to/install.mk -j8 all
#   make -f /path/to/install.mk -j8 package DESTDIR=/tmp/stage prefix=/usr \
#       TARBALL=/tmp/stage.tar
#
CC ?= cc
N ?= 200
DESTDIR ?=
prefix ?= /usr
TARBALL ?= $(DESTDIR)/../stage.tar
# Real builder uid/gid — pass from the benchmark/CI driver (`id -u` before wrapping).
# Do not use $(shell id -u) here: under pseudoroot/fakeroost/fakeroot that returns 0.
INSTALLER_UID ?= $(shell id -u)
INSTALLER_GID ?= $(shell id -g)
bindir = $(DESTDIR)$(prefix)/bin
libdir = $(DESTDIR)$(prefix)/lib
sbindir = $(DESTDIR)$(prefix)/sbin
datadir = $(DESTDIR)$(prefix)/share/pseudoroot-bench
devdir = $(DESTDIR)$(prefix)/dev

IDS := $(shell seq 0 $$(($(N) - 1)))

PROGS := $(addprefix build/app-,$(IDS))
LIBS := $(addprefix build/lib-,$(IDS))

INST_BINS := $(addprefix $(bindir)/app-,$(IDS))
INST_LIBS := $(addprefix $(libdir)/lib-,$(IDS))
INST_SBIN := $(addprefix $(sbindir)/app-,$(IDS))
INST_DATA := $(addprefix $(datadir)/lib-,$(IDS))
INST_DEVS := $(addprefix $(devdir)/null-,$(IDS))

.PHONY: all install package clean clean-install dirs

all: $(PROGS) $(LIBS)

build/app-%: build/app-%.c
	@mkdir -p build
	$(CC) -O1 -s -o $@ $<

build/lib-%: build/lib-%.c
	@mkdir -p build
	$(CC) -O1 -s -c -o $@.o $<
	$(CC) -O1 -s -shared -o $@ $@.o
	@rm -f $@.o

install: dirs $(INST_BINS) $(INST_LIBS) $(INST_SBIN) $(INST_DATA) $(INST_DEVS)

# install + tar archive (DESTDIR staging then pack, like `make install DESTDIR=…` + dist tarball).
package: install
	tar -C $(DESTDIR) --numeric-owner -cf $(TARBALL) usr

dirs:
	@install -d $(bindir) $(libdir) $(sbindir) $(datadir) $(devdir)

# Root-owned binaries and sbin copies.
$(bindir)/app-%: build/app-%
	install -m 755 -o 0 -g 0 $< $@

$(sbindir)/app-%: build/app-%
	install -m 755 -o 0 -g 0 $< $@

# Installer-owned libs and data (non-root package content).
$(libdir)/lib-%: build/lib-%
	install -m 644 -o $(INSTALLER_UID) -g $(INSTALLER_GID) $< $@

$(datadir)/lib-%: build/lib-%
	install -m 644 -o $(INSTALLER_UID) -g $(INSTALLER_GID) $< $@

# Root-owned device nodes (char, 1:3).
$(devdir)/null-%:
	@mkdir -p $(devdir)
	mknod -m 660 $@ c 1 3
	chown 0:0 $@

clean:
	rm -rf build

clean-install:
	rm -rf $(DESTDIR)

clean-all: clean clean-install
