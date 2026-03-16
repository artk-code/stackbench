import { MakerDeb } from "@electron-forge/maker-deb";
import { VitePlugin } from "@electron-forge/plugin-vite";

export default {
  packagerConfig: {
    asar: true,
    executableName: "stackbench",
  },
  makers: [
    new MakerDeb({
      options: {
        maintainer: "Stackbench Contributors",
        homepage: "https://github.com/artk/stackbench",
        categories: ["Development"],
      },
    }),
  ],
  plugins: [
    new VitePlugin({
      build: [
        {
          entry: "src/main.ts",
          config: "vite.main.config.ts",
        },
        {
          entry: "src/preload.ts",
          config: "vite.preload.config.ts",
        },
      ],
      renderer: [
        {
          name: "main_window",
          config: "vite.renderer.config.ts",
        },
      ],
    }),
  ],
};
