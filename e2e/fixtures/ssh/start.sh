#!/bin/sh
set -eu
ssh-keygen -A
exec /sbin/init
