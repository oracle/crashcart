#!/bin/bash
set -euo pipefail

http_proxy=${http_proxy:-}
https_proxy=${https_proxy:-}
ftp_proxy=${ftp_proxy:-}

PACKAGES=$(sed "s/\n/ /g" packages)

docker build -t crashcart-builder \
    --build-arg http_proxy="${http_proxy}" \
    --build-arg https_proxy="${https_proxy}" \
    --build-arg ftp_proxy="${ftp_proxy}" \
    builder

docker run --privileged --rm -i \
    -e "PACKAGES=${PACKAGES}" \
    -e http_proxy="${http_proxy}" \
    -e https_proxy="${https_proxy}" \
    -e ftp_proxy="${ftp_proxy}" \
    -v "${PWD}"/vol:/dev/crashcart crashcart-builder /dev/crashcart/build_crashcart.sh

mv -f vol/crashcart.img .
