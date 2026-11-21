FROM registry.fedoraproject.org/fedora:42
ARG ATSPI_REV=4199d3ea3ccdcaa175b5a06ed3ad8f1af498f38a
ARG TEST_UID=1000
COPY fedora-42.repo /etc/yum.repos.d/jam-fixture.repo
RUN dnf --disablerepo='*' --enablerepo=jam-fixture-base --enablerepo=jam-fixture-updates \
      install -y --setopt=install_weak_deps=False \
      git cmake ninja-build gcc-c++ extra-cmake-modules \
      qt6-qtbase-devel qt6-qtbase-private-devel qt6-qtwayland-devel \
      kf6-kwindowsystem-devel kf6-kcoreaddons-devel kwayland-devel kpipewire-devel \
      plasma-wayland-protocols-devel wayland-devel libxkbcommon-devel \
      kwin at-spi2-core gtk3 gtk4 libadwaita \
      python3 python3-pip python3-pyatspi python3-gobject python3-lxml python3-numpy python3-pillow \
      xorg-x11-server-Xvfb \
      ruby ruby-bundled-gems which libcap dbus-daemon dbus-tools pipewire wireplumber \
      mesa-dri-drivers mesa-vulkan-drivers dejavu-sans-fonts vulkan-loader \
    && dnf clean all \
    && if ! command -v ruby >/dev/null; then ln -s /usr/bin/ruby-mri /usr/local/bin/ruby; fi
# Fedora's compositor requests file capabilities outside Docker's bounding
# set. The virtual test compositor needs no real-time scheduling privilege.
RUN if [ -n "$(getcap /usr/bin/kwin_wayland)" ]; then setcap -r /usr/bin/kwin_wayland; fi
RUN python3 -m venv --system-site-packages /opt/atspi \
    && /opt/atspi/bin/pip install Flask==3.0.0 Appium-Python-Client==4.5.1 selenium==4.29.0 uv==0.13.5
COPY allow-gtk.patch /opt/allow-gtk.patch
COPY gtk_geometry.py /usr/local/share/selenium-webdriver-at-spi/gtk_geometry.py
COPY headless_screenshot.py /usr/local/share/selenium-webdriver-at-spi/headless_screenshot.py
COPY run-wayland.sh /usr/local/bin/jam-e2e-wayland-run
RUN chmod 755 /usr/local/bin/jam-e2e-wayland-run
COPY upstream /opt/atspi-src
RUN test "$(git -C /opt/atspi-src rev-parse HEAD)" = "$ATSPI_REV" \
    && /opt/atspi/bin/pip install -r /opt/atspi-src/requirements.txt \
    && touch /tmp/selenium-requirements-installed \
    && git -C /opt/atspi-src apply --unidiff-zero /opt/allow-gtk.patch \
    && ruby -c /opt/atspi-src/run.rb \
    && /opt/atspi/bin/python -m py_compile /opt/atspi-src/selenium-webdriver-at-spi.py /usr/local/share/selenium-webdriver-at-spi/gtk_geometry.py \
    && PATH="/opt/atspi/bin:$PATH" cmake -S /opt/atspi-src -B /opt/atspi-build -G Ninja \
         -DCMAKE_INSTALL_PREFIX=/usr/local -DBUILD_TESTING=OFF -DCMAKE_BUILD_TYPE=Release \
    && cmake --build /opt/atspi-build --parallel 1 \
    && cmake --install /opt/atspi-build
RUN useradd --create-home --uid "$TEST_UID" tester \
    && mkdir -p /tmp/driver-runtime && chmod 700 /tmp/driver-runtime \
    && chown tester:tester /tmp/driver-runtime
ENV PATH="/opt/atspi/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    XDG_RUNTIME_DIR=/tmp/driver-runtime \
    LIBGL_ALWAYS_SOFTWARE=1 USE_CUSTOM_BUS=1 \
    KWIN_COMPOSE=Q GTK_IM_MODULE=gtk-im-context-simple \
    TEST_WITH_CLEAN_HOME=0 TEST_WITH_VIDEO_RECORDER=0
RUN ruby -rfileutils -rlogger -rshellwords -rtmpdir -rnet/http -e 'puts RUBY_VERSION' \
    && which pip3 \
    && /opt/atspi/bin/flask --version
USER tester
WORKDIR /work
ENTRYPOINT ["jam-e2e-wayland-run"]
