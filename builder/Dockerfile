FROM alpine

ENV USER=root
ARG nversion=1.11.15
ARG nsha=57bebb9718c3e12dfed6ae5ac0aa6960d8cc73efb01aecd0e6d2854c48c39444
RUN sed -i -e 's/v3\.6/edge/g' /etc/apk/repositories
RUN echo http://dl-cdn.alpinelinux.org/alpine/edge/testing >> /etc/apk/repositories
# The __CYGWIN__ sed below is to work around permissions isssues when mounting
# a docker volume from darwin.
RUN echo 'nixbld:x:998:nobody' >> /etc/group && \
    apk add -U bash perl libressl bzip2 sqlite curl xz perl-dbd-sqlite perl-dbi libsodium gcc g++ make \
        perl-dev libressl-dev bzip2-dev sqlite-dev curl-dev xz-dev libsodium-dev linux-headers libbz2 libstdc++ \
        e2fsprogs e2fsprogs-extra perl-www-curl libseccomp-dev && \
    rm -rf /var/cache/apk/* && \
    curl -OL https://nixos.org/releases/nix/nix-${nversion}/nix-${nversion}.tar.bz2 && \
    echo "${nsha}  nix-${nversion}.tar.bz2" | sha256sum -c && \
    tar -xjf nix-${nversion}.tar.bz2 && \
    sed -i '1i#include <string.h>' nix-${nversion}/src/libutil/affinity.cc && \
    sed -i 's/#ifndef __CYGWIN__/#if 0/g' nix-${nversion}/src/libstore/build.cc  && \
    cd nix-${nversion} && \
    ./configure --localstatedir=/dev/crashcart/var --with-store-dir=/dev/crashcart/store && \
    make && \
    make install && \
    cd - && \
    rm -rf nix-${nversion}* && \
    apk del gcc g++ make perl-dev libressl-dev libressl-dev bzip2-dev sqlite-dev curl-dev xz-dev libsodium-dev linux-headers
