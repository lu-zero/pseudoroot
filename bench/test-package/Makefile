SRCS = $(wildcard file*.txt)
TARGETS = $(SRCS:.txt=.out)

all: $(TARGETS)

%.out: %.txt
	cp $< $@

clean:
	rm -f $(TARGETS)

.PHONY: all clean
