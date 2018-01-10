#!/bin/bash
set -euo pipefail

PACKAGES=${PACKAGES:-}

cd /dev/crashcart/
nix-channel --list | grep nixos-17.09 || nix-channel --add https://nixos.org/channels/nixos-17.09
nix-channel --update

# workaround for ncurses
sed -i 's;20170902;20180106;g' \
  ~/.nix-defexpr/channels/nixos-17.09/pkgs/development/libraries/ncurses/default.nix
sed -i 's;1cks4gsz4148jw6wpqia4w5jx7cfxr29g2kmpvp0ssmvwczh8dr4;27a178398314b81c27d54672b42b6bb4475c77e72f126dbedde8f8bf220d081e;g' \
  ~/.nix-defexpr/channels/nixos-17.09/pkgs/development/libraries/ncurses/default.nix

rm -f profile
nix-env -p profile -i ${PACKAGES}
rm -f crashcart.img
truncate -s 1G crashcart.img
mkfs.ext3 crashcart.img
mkdir -p out
mount -t ext2 -o loop crashcart.img out
ln -s "$(readlink -f profile)" out/profile
ln -s profile/bin out/bin
ln -s profile/sbin out/sbin
cp .crashcartrc out/
mkdir -p out/store
for deps in $(nix-store -qR profile); do
    cp -a  "${deps#/dev/crashcart/*}" out/store/
done
umount out
# We expect this to return 1
set +e
e2fsck -f crashcart.img
set -e
resize2fs -M crashcart.img
