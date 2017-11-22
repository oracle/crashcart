FROM ubuntu:16.04

ENV USER=root
ARG nversion=1.11.15
ARG nsha=57bebb9718c3e12dfed6ae5ac0aa6960d8cc73efb01aecd0e6d2854c48c39444
RUN apt-get update && apt-get -y install curl build-essential pkg-config autotools-dev dh-autoreconf libssl-dev libbz2-dev libsqlite3-dev libcurl4-openssl-dev liblzma-dev libgc-dev libdbi-perl libdbd-sqlite3-perl libwww-curl-perl libxml2 libxslt-dev libseccomp-dev \
    && apt-get clean && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*
RUN echo 'nixbld:x:998:nobody' >> /etc/group && \
    curl -OL https://nixos.org/releases/nix/nix-${nversion}/nix-${nversion}.tar.bz2 && \
    echo "${nsha}  nix-${nversion}.tar.bz2" | sha256sum -c && \
    tar -xjf nix-${nversion}.tar.bz2 && \
    cd nix-${nversion} && \
    ./configure --localstatedir=/dev/crashcart/var --with-store-dir=/dev/crashcart/store && \
    make && \
    make install && \
    cd - && \
    rm -rf nix-${nversion}*
