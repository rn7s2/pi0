import type { Configuration } from 'webpack';

import { rules } from './webpack.rules';
import { plugins } from './webpack.plugins';

export const mainConfig: Configuration = {
    /**
     * This is the main entry point for your application, it's the first file
     * that runs in the main process.
     */
    entry: './src/index.ts',
    // Put your normal webpack config below here
    module: {
        rules,
    },
    plugins,
    resolve: {
        extensions: ['.js', '.ts', '.jsx', '.tsx', '.css', '.json'],
        // The @pi0/native addon is a `file:` dependency, which npm installs as a
        // symlink into node_modules. Keep the node_modules path (don't resolve the
        // symlink to native/) so the asset-relocator + node-loader rules — which key
        // off a `node_modules/` path segment — pick up the generated .node.
        symlinks: false,
    },
};
