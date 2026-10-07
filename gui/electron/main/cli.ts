import { Option, program } from 'commander';

program
  .option('-p, --path <path>', 'directory containing the Rust backend')
  .option('--rust-server <path>', 'explicit Rust backend executable')
  .option('--config <path>', 'SlimeVR YAML configuration file')
  .option('--no-server', 'connect without starting a backend')
  .addOption(
    new Option('--log-level <level>', 'backend log verbosity').choices([
      'error',
      'warn',
      'info',
      'debug',
      'trace',
    ])
  )
  .option('-s, --steam', 'steam mode')
  .option(
    '--skip-server-if-running',
    'gui will not launch the server if it is already running'
  )
  .allowUnknownOption()
  // Allow passing arguments to Electron.
  .allowExcessArguments();

program.parse(process.argv);
export const options = program.opts();
