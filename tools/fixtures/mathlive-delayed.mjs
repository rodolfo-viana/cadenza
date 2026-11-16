// Exercise the real loader's disposal guard without a production test hook.
export * from '../../ui/vendor/mathlive/mathlive.min.mjs?original-module';
await new Promise((resolve) => setTimeout(resolve, 350));
