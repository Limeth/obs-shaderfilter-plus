# OBS ShaderFilter Plus
OBS ShaderFilter Plus is a plugin for Open Broadcaster Software.
It can be used to apply effects to sources using manually created GLSL/HLSL shaders.
Add a filter to a source by right-clicking a source, going to `Filters`, and adding `ShaderFilter Plus`.

Shaders are executed using OpenGL (GLSL shaders) or DirectX (HLSL shaders),
depending on your platform.
OBS on Windows usually uses DirectX, but can be compiled with OpenGL support.
OBS on Linux uses OpenGL.

When OBS is compiled with OpenGL support, it performs primitive translation of HLSL sources to GLSL.
Therefore, if you would like your shaders to run with both graphics APIs, you should write your sources in HLSL.
However, if you only care about using them on Linux, for example, GLSL is fine.

## Installation
1. Download the latest binary for your platform from [the Releases page](https://github.com/Limeth/obs-shaderfilter-plus/releases).
    * On Windows, download the file ending with `_windows_x64.dll`
    * On Linux, download the file ending with `_linux_x64.so`
2. Place it in the OBS plugin directory:
    * On Windows, that is usually `C:\Program Files\obs-studio\obs-plugins\64bit`
    * On Linux, that is usually `/usr/lib/obs-plugins`

## Usage Guide
The structure of a shader is simple. All, that is required, is the following `render` function.

```hlsl
float4 render(float2 uv) {
    // sample the source texture and return its color to be displayed
    return image.Sample(builtin_texture_sampler, uv);
}
```

### Builtin Variables
Every shader loaded by this plugin has access to the following uniform variables.

```hlsl
uniform texture2d image;                         // the source texture (the image we are filtering)
uniform int       builtin_frame;                 // the current frame number
uniform float     builtin_framerate;             // the current output framerate
uniform float     builtin_elapsed_time;          // the current elapsed time
uniform float     builtin_elapsed_time_previous; // the elapsed time in the previous frame
uniform int2      builtin_uv_size;               // the source dimensions

sampler_state     builtin_texture_sampler { ... }; // a texture sampler with linear filtering
```

#### On-Request Builtin Variables
These uniform variables will be assigned data by the plugin.
If they are not defined, they do not use up processing resources.

```hlsl
uniform texture2d builtin_texture_fft_<NAME>;          // audio output frequency spectrum
uniform texture2d builtin_texture_fft_<NAME>_previous; // output from the previous frame (requires builtin_texture_fft_<NAME> to be defined)
```

Builtin FFT variables have specific properties. See the the section below on properties.

Example:

```hlsl
#pragma shaderfilter set myfft__mix__description Main Mix/Track
#pragma shaderfilter set myfft__channel__description Main Channel
#pragma shaderfilter set myfft__dampening_factor_attack__step 0.0001
#pragma shaderfilter set myfft__dampening_factor_attack__default 0.05
#pragma shaderfilter set myfft__dampening_factor_release 0.0001
uniform texture2d builtin_texture_fft_myfft;
```

See the `examples` directory for more examples.

#### Custom Variables
These uniform variables may be used to let the user provide values to the shader using the OBS UI.
The allowed types are:
* `bool`: A boolean variable
* `int`: A signed 32-bit integer variable
* `float`: A single precision floating point variable
* `float4`/`vec4`: A color variable, shown as a color picker in the UI

Example:

```hlsl
#pragma shaderfilter set my_color__description My Color
#pragma shaderfilter set my_color__default 7FFF00FF
uniform float4 my_color;
```

See the `examples` directory for more examples.

### Defining Properties in the Source Code
This plugin uses a simple preprocessor to process `#pragma shaderfilter` macros.
It is not a fully-featured C preprocessor. It is executed before the shader is
compiled. The shader compilation includes an actual C preprocessing step.
Do not expect to be able to use macros within `#pragma shaderfilter`.

Most properties can be specified in the shader source code. The syntax is as follows:
```
#pragma shaderfilter set <PROPERTY> <VALUE>
```

#### Universal Properties
These properties can be applied to any user-defined uniform variable.
* `default`: The default value of the uniform variable.
* `description`: The user-facing text describing the variable. Displayed in the OBS UI.

#### Integer Properties
* `min` (integer): The minimum allowed value
* `max` (integer): The maximum allowed value
* `step` (integer): The stride when changing the value
* `slider` (true/false): Whether to display a slider or not

#### Float Properties
* `min` (float): The minimum allowed value
* `max` (float): The maximum allowed value
* `step` (float): The stride when changing the value
* `slider` (true/false): Whether to display a slider or not

#### FFT Properties
* `mix`: The Mix/Track number corresponding to checkboxes in OBS' `Advanced Audio Properties`
* `channel`: The channel number (0 = Left, 1 = Right for stereo)
* `dampening_factor_attack`: The linear interpolation coefficient (in percentage) used to blend the previous FFT sample with the current sample, if it is larger than the previous
* `dampening_factor_release`: The linear interpolation coefficient (in percentage) used to blend the previous FFT sample with the current sample, if it is lesser than the previous



## Planned Features
* Access to raw audio signal, without FFT
* Specifying textures by a path to an image file

## Development
### Building
#### Windows
1. Install Rust by following instructions at https://rustup.rs/
2. Install CLang: Download and install the official pre-built binary from
[LLVM download page](http://releases.llvm.org/download.html)
3. Compile OBS by following [these instructions](https://github.com/obsproject/obs-studio/wiki/Install-Instructions#windows-build-directions)
    * This will require the installation of _Visual Studio Build Tools 2019_, _Visual Studio Community 2019_ and _CMake_.
        * Visual Studio Build Tools requirements:
            1. `MSVC v142 - VS 2019 C++ x64/x86 build tools (v14.25)` or later
            2. `MSVC v142 - VS 2019 C++ x64/x86 Spectre-mitigated libs (v14.25)` or later
        * Visual Studio Community 2019 requirements:
            1. `MSVC v142 - VS 2019 C++ x64/x86 build tools (v14.25)` or later
            2. `MSVC v142 - VS 2019 C++ x64/x86 Spectre-mitigated libs (v14.25)` or later
            3. `C++ ATL for latest v142 build tools (x86 & x64)` or later
            4. `C++ ATL for latest v142 build tools with Spectre Mitigations (x86 & x64)` or later
    * To configure OBS via `cmake-gui`, set the following variables:
        * `DISABLE_PYTHON=TRUE`, unless you are not getting errors while trying to build with Python enabled
4. Compile OBS ShaderFilter Plus, replace `<OBS_BUILD_DIR>` with the path to the directory where you built OBS:
    ```bat
    set RUSTFLAGS=-L native=<OBS_BUILD_DIR>\libobs\Debug
    cargo build --release
    ```
5. Move `target/release/libobs_shaderfilter_plus.dll` to the OBS plugin directory.
#### Linux
1. Install Rust (the package manager Cargo should be bundled with it)
2. Clone this repository and open it in the terminal
3. Compile using `cargo build --release`
4. Move `target/release/libobs_shaderfilter_plus.so` to the OBS plugin directory.

### Tips on building OBS (fish shell, Ubuntu)
These steps should not be necessary if you just want to compile OBS ShaderFilter Plus from source.

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
