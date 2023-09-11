# just-gl

Run an OpenGL application on Linux, and nothing more.

## Kernel Setup for Nvidia Cards

I had to add `nvidia_drm.modeset=1` as a kernel commandline parameter. On Arch this was done by editing `/boot/loader/entries/arch.conf`.

## Reading References

https://gitlab.freedesktop.org/mesa/kmscube

https://github.com/dvdhrm/docs/tree/master/drm-howto

https://zamundaaa.github.io/wayland/2023/03/10/porting-away-from-gbm-surface.html

## Remote Environment Setup

Development can take place much faster if you develop in your local environment, and run/test on a remote machine without X11 or Wayland.

It's also necessary if you're developing on a non-linux platform, as libdrm and others are highly linux-specific.

I (bschwind) am currently using Sublime Text with this remote setup:

* Install the [Rsync SSH Package](https://packagecontrol.io/packages/Rsync%20SSH)
* Install `rsync` on both your local machine and the target remote machine
* Configure the `Rsync SSH` sublime package with your target machine's hostname, repo location, etc. My config looks like this:

<details><summary>Sublime RSync SSH Config</summary>
<p>

```
{
	"folders":
	[
		{
			"path": "/Users/brian/projects/tonari/just-gl",
		}
	],
	"settings":
	{
		"rsync_ssh":
		{
			"excludes":
			[
				".git*",
				"_build",
				"blib",
				"Build"
			],
			"options":
			[
				"--delete"
			],
			"remotes":
			{
				"/Users/brian/projects/tonari/just-gl":
				[
					{
						"command": "rsync",
						"enabled": 1,
						"excludes":
						[
						],
						"options":
						[
						],
						"remote_host": "tonarchi-test-machine.tonari.wg",
						"remote_path": "/home/tonari/projects/just-gl",
						"remote_port": 22,
						"remote_post_command": "",
						"remote_pre_command": "",
						"remote_user": "tonari"
					}
				]
			},
			"sync_on_save": true
		}
	},
}

```
</p>
</details>

* I have a passwordless SSH key to use to SSH into the target remote machine
* The target remote machine has my public SSH key added to `~/.ssh/authorized_keys`
* On the remote machine, I use `cargo watch` (`cargo install cargo-watch`) to run a command which will execute every time files change on the file system (which happens every time you save files on your local machine, thanks to rsync):
  * `$ cargo watch -x 'clippy --all-targets -- -D warnings'`
* Develop locally, and receive compilation results from an SSH session on the remote machine (running `cargo watch`)
