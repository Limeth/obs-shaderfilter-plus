# Building


# Development
Ensure OBS is uninstalled using:
```fish
sudo apt remove obs-studio
```

Compile OBS using:
```fish
cmake -DUNIX_STRUCTURE=1 -DCMAKE_INSTALL_PREFIX=/usr -DBUILD_BROWSER=ON -DCEF_ROOT_DIR="../../cef_binary_3770_linux64" ..; and make -j24; and sudo checkinstall --default --pkgname=obs-studio --fstrans=no --backup=no --pkgversion=(date +%Y%m%d)"-git" --deldoc=yes
```

Then recompile and install using:
```
make -j24; and sudo make install
```
