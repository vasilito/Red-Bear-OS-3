PROGRAMS = \
	test_signalfd_wayland \
	test_timerfd_qt6 \
	test_eventfd_qt6 \
	test_shm_open_qt6 \
	test_sem_open_qt6 \
	test_waitid_qt6

CC ?= cc
CPPFLAGS ?=
CFLAGS ?= -O2 -Wall -Wextra -Werror
LDFLAGS ?=

.PHONY: all clean

all: $(PROGRAMS)

%: %.c
	$(CC) $(CPPFLAGS) $(CFLAGS) $< -o $@ $(LDFLAGS)

clean:
	rm -f $(PROGRAMS)
