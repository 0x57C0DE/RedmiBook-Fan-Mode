PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
UNITDIR ?= /etc/systemd/system
MODPROBEDIR ?= /etc/modprobe.d
PPD_DROPIN_DIR ?= /etc/systemd/system/power-profiles-daemon.service.d

NAME = redmibook-fan-mode
BINARY = target/release/$(NAME)
SERVICE = $(NAME).service
BLACKLIST = blacklist-bitland-mifs.conf
PPD_DROPIN = block-platform-profile.conf

.PHONY: build check install uninstall install-workarounds uninstall-workarounds

build:
	cargo build --release --locked

check:
	cargo test --locked
	cargo clippy --locked --all-targets -- -D warnings

install:
	@test -x $(BINARY) || { echo "run 'make build' first, as your user"; exit 1; }
	install -D -m 755 $(BINARY) $(DESTDIR)$(BINDIR)/$(NAME)
	install -d $(DESTDIR)$(UNITDIR)
	sed 's|@BINDIR@|$(BINDIR)|' systemd/$(SERVICE).in > $(DESTDIR)$(UNITDIR)/$(SERVICE)
	chmod 644 $(DESTDIR)$(UNITDIR)/$(SERVICE)
	systemctl daemon-reload
	systemctl enable $(SERVICE)
	systemctl restart $(SERVICE)

uninstall:
	-systemctl disable --now $(SERVICE)
	rm -f $(DESTDIR)$(UNITDIR)/$(SERVICE) $(DESTDIR)$(BINDIR)/$(NAME)
	systemctl daemon-reload

install-workarounds:
	install -D -m 644 modprobe.d/$(BLACKLIST) $(DESTDIR)$(MODPROBEDIR)/$(BLACKLIST)
	install -D -m 644 systemd/$(PPD_DROPIN) $(DESTDIR)$(PPD_DROPIN_DIR)/$(PPD_DROPIN)
	systemctl daemon-reload
	systemctl restart power-profiles-daemon.service
	-modprobe -r bitland_mifs_wmi

uninstall-workarounds:
	rm -f $(DESTDIR)$(MODPROBEDIR)/$(BLACKLIST) $(DESTDIR)$(PPD_DROPIN_DIR)/$(PPD_DROPIN)
	-rmdir $(DESTDIR)$(PPD_DROPIN_DIR)
	systemctl daemon-reload
	systemctl restart power-profiles-daemon.service
