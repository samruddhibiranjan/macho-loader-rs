NAME            := execvm_example
CC              := gcc
SRCS_DIR        := srcs
OBJS_DIR        := .objs
INCS_DIR        := incs
LIBEXECVM_DIR   := ..
LIBVECTOR_DIR   := libvector
LIBTLV_DIR      := ../cxx_libtlv

DEBUG ?= 0
ifeq ($(DEBUG), 1)
    BUILD_MODE := debug
    CARGO_FLAGS :=
    CFLAGS_MODE := \
			-D DEBUG                    \
			-Og                         \
			-Wall                       \
			-Werror                     \
			-Wextra                     \
			-pedantic                   \
			-fsanitize=address          \
			-fsanitize=undefined        \
			-fno-omit-frame-pointer     \
			-fno-optimize-sibling-calls
else
    BUILD_MODE := release
    CARGO_FLAGS := --release
    CFLAGS_MODE := -O3
endif

RUST_LIB := $(LIBEXECVM_DIR)/target/$(BUILD_MODE)/libexecvm.dylib
VECTOR_LIB := $(LIBVECTOR_DIR)/libvector.a

CFLAGS := \
    -D WAIT_FOR_CHILD \
    $(CFLAGS_MODE)

SRCS := \
		spawn.c \
    main.c

SRCS_OBJS := $(patsubst %.c,$(OBJS_DIR)/%.o,$(SRCS))

$(OBJS_DIR)/%.o:$(SRCS_DIR)/%.c
	@mkdir -vp $(dir $@)
	$(CC) \
	    $(CFLAGS) \
	    -MMD \
	    -MP \
	    -o $@ \
	    -c $< \
	    -fPIC \
	    -I $(INCS_DIR)

.DEFAULT_GOAL := all

all: $(NAME)

g:
	$(MAKE) DEBUG=1

-include $(SRCS_OBJS:.o=.d)

$(NAME): $(SRCS_OBJS) $(RUST_LIB)
	$(MAKE) -C $(LIBVECTOR_DIR)
	$(CC) $^ $(CFLAGS) \
	    -I $(INCS_DIR) \
	    -L $(LIBEXECVM_DIR)/target/$(BUILD_MODE) -lexecvm \
			-L $(LIBVECTOR_DIR) -lvector \
	    -o $(NAME)

$(RUST_LIB): $(LIBEXECVM_DIR)/Cargo.toml $(LIBEXECVM_DIR)/src/*.rs
	$(MAKE) -C $(LIBTLV_DIR)
	cargo build --manifest-path $(LIBEXECVM_DIR)/Cargo.toml $(CARGO_FLAGS)

clean:
	$(MAKE) clean -C $(LIBTLV_DIR)
	$(MAKE) clean -C $(LIBVECTOR_DIR)
	rm -rf target
	rm -rf $(LIBEXECVM_DIR)/target
	rm -rf build
	rm -rf *.dSYM
	rm -rf $(OBJS_DIR)

fclean: clean
	$(MAKE) fclean -C $(LIBTLV_DIR)
	$(MAKE) fclean -C $(LIBVECTOR_DIR)
	rm -rf $(NAME)

re: fclean all

.PHONY: all clean release fclean re g