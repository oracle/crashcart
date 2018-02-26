# `crashcart` - microcontainer debugging tool #

![crashcart](https://github.com/oracle/crashcart/raw/master/crashcart.png
"crashcart")

## What is `crashcart`? ##

`crashcart` is a simple command line utility that lets you sideload an image
with linux binaries into an existing container.

## Building `crashcart` ##

[![wercker status](https://app.wercker.com/status/3b1da922588f5550faca49a356013e52/s/master "wercker status")](https://app.wercker.com/project/byKey/3b1da922588f5550faca49a356013e52)

Install rust:

    curl https://sh.rustup.rs -sSf | sh
    rustup toolchain install stable-x86_64-unknown-linux-gnu
    rustup default stable-x86_64-unknown-linux-gnu # for stable
    rustup target install x86_64-unknown-linux-musl # for stable
    rustup toolchain install nightly-x86_64-unknown-linux-gnu
    rustup default nightly-x86_64-unknown-linux-gnu # for nightly
    rustup target install x86_64-unknown-linux-musl # for nightly

Building can be done via build.sh:

    build.sh

By default, build.sh builds a dynamic binary using gnu. To build a static
binary, set `TARGET` to `x86_64-unknown-linux-musl`:

    TARGET=x86_64-unknown-linux-musl ./build.sh

## Building `crashcart.img` ##

Image build dependencies:

    sudo
    docker

`crashcart` will load binaries from an image file into a running container. To
build the image, you just need docker installed and then you can use
build_image.sh:

    build_image.sh

The build image script will build a `crashcart_builder` image using the
dockerfile in the builder directory. It will then run this builder as a
privileged container. It needs to be privileged because the image is created by
loopback mounting an ext3 filesystem and copying files in. It may be possible
to do this without root privileges using something like e2tools, but these have
not been packaged for alpine.

The `crashcart_builder` will take a very long time the first time it is run.
The relocated binaries are built from source via the nix package manager, and
the toolchain needs to be built from scratch. Later builds should go much more
quickly because the nix store is cached in a in the vol directory and bind
mounted into the builder.

To add to the list of packages in the resulting image, simply add the package
names to the packages file before building. Packages are installed via the
nix-env tool. An up-to-date list of nix packages can be searched
[here](https://nixos.org/nixos/packages.html).

## Using `crashcart` ##

To enter a container and run `crashcart`'s bash just pass the container id:

    sudo ./crashcart $ID

$ID can be the container id of a `docker` or `rkt` container, or the pid of any
process running inside a container.

To run another command from the `crashcart` image, pass the full path:

    sudo ./crashcart $ID /dev/crashcart/bin/tcpdump

To use docker-exec instead of entering the namespaces via `crashcart`'s
internal namespace handling, use the -e flag (NOTE: that this requires $ID to be
a docker container id):

    sudo ./crashcart -e $ID

## Manually Running Binaries from the `crashcart` Image ##

To manually mount the `crashcart` image into a container, use the -m flag.

    sudo ./crashcart -m $ID

To manually unmount the `crashcart` image from a container, use the -u flag.

    sudo ./crashcart -u $ID

Once you have manually mounted the image, you can use `docker exec` or
`nsenter` to run things inside the container.  `crashcart` locates its binaries
in `/dev/crashcart/bin` or `/dev/crashcart/sbin`. To execute
`tcpdump` for example, you can use:

    docker exec -it $CONTAINER_ID /dev/crashcart/bin/tcpdump

To run a shell with the all of `crashcart`'s utilities available in the path, you
can use:

    docker exec -it $CONTAINER_ID -- \
    /dev/crashcart/profile/bin/bash --rcfile /dev/crashcart/.crashcartrc -i

You can also do an equivalent command using `nsenter`:

    sudo nsenter -m -u -i -n -p -t $PID -- \
    /dev/crashcart/profile/bin/bash --rcfile /dev/crashcart/.crashcartrc -i

Note that if you are using user namespaces you might have to specify -U. You
also can use -S and -G to use a different user or group id in the container.

`crashcart` leaves the image mounted as a loopback device. If there are no
containers still using the `crashcart` image, you can remove the device as
follows:

    sudo losetup -d `readlink crashcart.img.lnk`; sudo rm crashcart.img.lnk

## Known Issues ##

`crashcart` doesn't work with user namespaces prior to kernel 4.8. In earlier
versions of the kernel, when you attempt to mount a device inside a mount
namespace that is a child of a user namespace, the kernel returns EPERM. The
logic was changed in 4.8 so that it is possible as long as the caller of mount
is in the init userns.

## TODO ##

* add functionality to run image with crashcart mount using docker run -v
* temporarily remount /dev in the container rw if it is ro
* allow user to set uid and gid in the container

## Contributing ##

`crashcart` is an open source project. See [CONTRIBUTING](CONTRIBUTING.md) for
details.

Oracle gratefully acknowledges the contributions to `crashcart` that have been made
by the community.

## Getting in touch ##

The best way to get in touch is Slack.

Click [here](https://join.slack.com/t/oraclecontainertools/shared_invite/enQtMzIwNzg3NDIzMzE5LTIwMjZlODllMWRmNjMwZGM1NGNjMThlZjg3ZmU3NDY1ZWU5ZGJmZWFkOTBjNzk0ODIxNzQ2ODUyNThiNmE0MmI) to join the the [Oracle Container Tools workspace](https://oraclecontainertools.slack.com).

Then join the [Crashcart channel](https://oraclecontainertools.slack.com/messages/C8CJ5M9ML).

## License ##

Copyright (c) 2017, Oracle and/or its affiliates. All rights reserved.

`crashcart` is dual licensed under the Universal Permissive License 1.0 and the
Apache License 2.0.

See [LICENSE](LICENSE.txt) for more details.
