import swc from 'rollup-plugin-swc';
import nodeResolve from '@rollup/plugin-node-resolve';
import commonjs from "@rollup/plugin-commonjs";

export default {
    input: 'qr.js',
    output: {
        file: 'qr.min.js',
        format: 'es',
    },
    plugins: [
        swc({
            jsc: {
                target: 'es2020',
                minify: {
                    compress: true,
                    mangle: true,
                }
            },
            minify: true,
        }),
        nodeResolve(),
        commonjs(),
    ],
};
