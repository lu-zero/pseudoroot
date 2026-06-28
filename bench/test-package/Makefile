# Simple Makefile for benchmarking parallel builds

SRCS = file1.txt file2.txt file3.txt file4.txt file5.txt file6.txt file7.txt file8.txt
TARGETS = $(SRCS:.txt=.out)

all: $(TARGETS)

%.out: %.txt
	cp $< $@

clean:
	rm -f $(TARGETS)

.PHONY: all clean
