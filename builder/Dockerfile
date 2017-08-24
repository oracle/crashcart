FROM alpine

ENV USER=root
ARG nversion=1.11.6
ARG nsha=c3992530762322aa3c23a5d2c6df20a43cb255bc7969c4cd016f6a4262ca843c
run sed -i -e 's/v3\.6/edge/g' /etc/apk/repositories
run echo http://dl-cdn.alpinelinux.org/alpine/edge/testing >> /etc/apk/repositories
RUN echo 'nixbld:x:998:nobody' >> /etc/group && \
    apk add -U bash perl libressl bzip2 sqlite curl xz perl-dbd-sqlite perl-dbi libsodium gcc g++ make \
        perl-dev libressl-dev bzip2-dev sqlite-dev curl-dev xz-dev libsodium-dev linux-headers libbz2 libstdc++ \
        e2fsprogs e2fsprogs-extra perl-www-curl && \
    rm -rf /var/cache/apk/* && \
    curl -OL https://nixos.org/releases/nix/nix-${nversion}/nix-${nversion}.tar.bz2 && \
    echo "${nsha}  nix-${nversion}.tar.bz2" | sha256sum -c && \
    tar -xjf nix-${nversion}.tar.bz2 && \
    sed -i '1i#include <string.h>' nix-${nversion}/src/libutil/affinity.cc && \
    cd nix-${nversion} && \
    ./configure --localstatedir=/dev/crashcart/var --with-store-dir=/dev/crashcart/store && \
    make && \
    make install && \
    cd - && \
    rm -rf nix-${nversion}* && \
    apk del gcc g++ make perl-dev libressl-dev libressl-dev bzip2-dev sqlite-dev curl-dev xz-dev libsodium-dev linux-headers
RUN nix-channel --add https://nixos.org/channels/nixos-16.09 && \
    nix-channel --update
