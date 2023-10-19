#!/usr/bin/ruby

# A script to statically list syscalls used by a given binary.
#
# Syntax:  list-syscalls.rb <binary> [--only-used-syscalls]
#
# NOTE:    For accurate results, build the binary with musl and LTO enabled.
# Example: ./polkadot/scripts/list-syscalls/list-syscalls.rb target/x86_64-unknown-linux-musl/production/polkadot-prepare-worker --only-used-syscalls
#
# Author:  @koute
# Source:  https://gist.github.com/koute/166f82bfee5e27324077891008fca6eb

require 'shellwords'
require 'set'

SYNTAX_STRING = 'Syntax: list-syscalls.rb <binary> [--only-used-syscalls]'.freeze

# Generated from `libc` using the following regex:
#   'pub const SYS_([a-z0-9_]+): ::c_long = (\d+);'
#   '    \2 => "\1",'
SYSCALLS = {
  0 => 'read',
  1 => 'write',
  2 => 'open',
  3 => 'close',
  4 => 'stat',
  5 => 'fstat',
  6 => 'lstat',
  7 => 'poll',
  8 => 'lseek',
  9 => 'mmap',
  10 => 'mprotect',
  11 => 'munmap',
  12 => 'brk',
  13 => 'rt_sigaction',
  14 => 'rt_sigprocmask',
  15 => 'rt_sigreturn',
  16 => 'ioctl',
  17 => 'pread64',
  18 => 'pwrite64',
  19 => 'readv',
  20 => 'writev',
  21 => 'access',
  22 => 'pipe',
  23 => 'select',
  24 => 'sched_yield',
  25 => 'mremap',
  26 => 'msync',
  27 => 'mincore',
  28 => 'madvise',
  29 => 'shmget',
  30 => 'shmat',
  31 => 'shmctl',
  32 => 'dup',
  33 => 'dup2',
  34 => 'pause',
  35 => 'nanosleep',
  36 => 'getitimer',
  37 => 'alarm',
  38 => 'setitimer',
  39 => 'getpid',
  40 => 'sendfile',
  41 => 'socket',
  42 => 'connect',
  43 => 'accept',
  44 => 'sendto',
  45 => 'recvfrom',
  46 => 'sendmsg',
  47 => 'recvmsg',
  48 => 'shutdown',
  49 => 'bind',
  50 => 'listen',
  51 => 'getsockname',
  52 => 'getpeername',
  53 => 'socketpair',
  54 => 'setsockopt',
  55 => 'getsockopt',
  56 => 'clone',
  57 => 'fork',
  58 => 'vfork',
  59 => 'execve',
  60 => 'exit',
  61 => 'wait4',
  62 => 'kill',
  63 => 'uname',
  64 => 'semget',
  65 => 'semop',
  66 => 'semctl',
  67 => 'shmdt',
  68 => 'msgget',
  69 => 'msgsnd',
  70 => 'msgrcv',
  71 => 'msgctl',
  72 => 'fcntl',
  73 => 'flock',
  74 => 'fsync',
  75 => 'fdatasync',
  76 => 'truncate',
  77 => 'ftruncate',
  78 => 'getdents',
  79 => 'getcwd',
  80 => 'chdir',
  81 => 'fchdir',
  82 => 'rename',
  83 => 'mkdir',
  84 => 'rmdir',
  85 => 'creat',
  86 => 'link',
  87 => 'unlink',
  88 => 'symlink',
  89 => 'readlink',
  90 => 'chmod',
  91 => 'fchmod',
  92 => 'chown',
  93 => 'fchown',
  94 => 'lchown',
  95 => 'umask',
  96 => 'gettimeofday',
  97 => 'getrlimit',
  98 => 'getrusage',
  99 => 'sysinfo',
  100 => 'times',
  101 => 'ptrace',
  102 => 'getuid',
  103 => 'syslog',
  104 => 'getgid',
  105 => 'setuid',
  106 => 'setgid',
  107 => 'geteuid',
  108 => 'getegid',
  109 => 'setpgid',
  110 => 'getppid',
  111 => 'getpgrp',
  112 => 'setsid',
  113 => 'setreuid',
  114 => 'setregid',
  115 => 'getgroups',
  116 => 'setgroups',
  117 => 'setresuid',
  118 => 'getresuid',
  119 => 'setresgid',
  120 => 'getresgid',
  121 => 'getpgid',
  122 => 'setfsuid',
  123 => 'setfsgid',
  124 => 'getsid',
  125 => 'capget',
  126 => 'capset',
  127 => 'rt_sigpending',
  128 => 'rt_sigtimedwait',
  129 => 'rt_sigqueueinfo',
  130 => 'rt_sigsuspend',
  131 => 'sigaltstack',
  132 => 'utime',
  133 => 'mknod',
  134 => 'uselib',
  135 => 'personality',
  136 => 'ustat',
  137 => 'statfs',
  138 => 'fstatfs',
  139 => 'sysfs',
  140 => 'getpriority',
  141 => 'setpriority',
  142 => 'sched_setparam',
  143 => 'sched_getparam',
  144 => 'sched_setscheduler',
  145 => 'sched_getscheduler',
  146 => 'sched_get_priority_max',
  147 => 'sched_get_priority_min',
  148 => 'sched_rr_get_interval',
  149 => 'mlock',
  150 => 'munlock',
  151 => 'mlockall',
  152 => 'munlockall',
  153 => 'vhangup',
  154 => 'modify_ldt',
  155 => 'pivot_root',
  156 => '_sysctl',
  157 => 'prctl',
  158 => 'arch_prctl',
  159 => 'adjtimex',
  160 => 'setrlimit',
  161 => 'chroot',
  162 => 'sync',
  163 => 'acct',
  164 => 'settimeofday',
  165 => 'mount',
  166 => 'umount2',
  167 => 'swapon',
  168 => 'swapoff',
  169 => 'reboot',
  170 => 'sethostname',
  171 => 'setdomainname',
  172 => 'iopl',
  173 => 'ioperm',
  174 => 'create_module',
  175 => 'init_module',
  176 => 'delete_module',
  177 => 'get_kernel_syms',
  178 => 'query_module',
  179 => 'quotactl',
  180 => 'nfsservctl',
  181 => 'getpmsg',
  182 => 'putpmsg',
  183 => 'afs_syscall',
  184 => 'tuxcall',
  185 => 'security',
  186 => 'gettid',
  187 => 'readahead',
  188 => 'setxattr',
  189 => 'lsetxattr',
  190 => 'fsetxattr',
  191 => 'getxattr',
  192 => 'lgetxattr',
  193 => 'fgetxattr',
  194 => 'listxattr',
  195 => 'llistxattr',
  196 => 'flistxattr',
  197 => 'removexattr',
  198 => 'lremovexattr',
  199 => 'fremovexattr',
  200 => 'tkill',
  201 => 'time',
  202 => 'futex',
  203 => 'sched_setaffinity',
  204 => 'sched_getaffinity',
  205 => 'set_thread_area',
  206 => 'io_setup',
  207 => 'io_destroy',
  208 => 'io_getevents',
  209 => 'io_submit',
  210 => 'io_cancel',
  211 => 'get_thread_area',
  212 => 'lookup_dcookie',
  213 => 'epoll_create',
  214 => 'epoll_ctl_old',
  215 => 'epoll_wait_old',
  216 => 'remap_file_pages',
  217 => 'getdents64',
  218 => 'set_tid_address',
  219 => 'restart_syscall',
  220 => 'semtimedop',
  221 => 'fadvise64',
  222 => 'timer_create',
  223 => 'timer_settime',
  224 => 'timer_gettime',
  225 => 'timer_getoverrun',
  226 => 'timer_delete',
  227 => 'clock_settime',
  228 => 'clock_gettime',
  229 => 'clock_getres',
  230 => 'clock_nanosleep',
  231 => 'exit_group',
  232 => 'epoll_wait',
  233 => 'epoll_ctl',
  234 => 'tgkill',
  235 => 'utimes',
  236 => 'vserver',
  237 => 'mbind',
  238 => 'set_mempolicy',
  239 => 'get_mempolicy',
  240 => 'mq_open',
  241 => 'mq_unlink',
  242 => 'mq_timedsend',
  243 => 'mq_timedreceive',
  244 => 'mq_notify',
  245 => 'mq_getsetattr',
  246 => 'kexec_load',
  247 => 'waitid',
  248 => 'add_key',
  249 => 'request_key',
  250 => 'keyctl',
  251 => 'ioprio_set',
  252 => 'ioprio_get',
  253 => 'inotify_init',
  254 => 'inotify_add_watch',
  255 => 'inotify_rm_watch',
  256 => 'migrate_pages',
  257 => 'openat',
  258 => 'mkdirat',
  259 => 'mknodat',
  260 => 'fchownat',
  261 => 'futimesat',
  262 => 'newfstatat',
  263 => 'unlinkat',
  264 => 'renameat',
  265 => 'linkat',
  266 => 'symlinkat',
  267 => 'readlinkat',
  268 => 'fchmodat',
  269 => 'faccessat',
  270 => 'pselect6',
  271 => 'ppoll',
  272 => 'unshare',
  273 => 'set_robust_list',
  274 => 'get_robust_list',
  275 => 'splice',
  276 => 'tee',
  277 => 'sync_file_range',
  278 => 'vmsplice',
  279 => 'move_pages',
  280 => 'utimensat',
  281 => 'epoll_pwait',
  282 => 'signalfd',
  283 => 'timerfd_create',
  284 => 'eventfd',
  285 => 'fallocate',
  286 => 'timerfd_settime',
  287 => 'timerfd_gettime',
  288 => 'accept4',
  289 => 'signalfd4',
  290 => 'eventfd2',
  291 => 'epoll_create1',
  292 => 'dup3',
  293 => 'pipe2',
  294 => 'inotify_init1',
  295 => 'preadv',
  296 => 'pwritev',
  297 => 'rt_tgsigqueueinfo',
  298 => 'perf_event_open',
  299 => 'recvmmsg',
  300 => 'fanotify_init',
  301 => 'fanotify_mark',
  302 => 'prlimit64',
  303 => 'name_to_handle_at',
  304 => 'open_by_handle_at',
  305 => 'clock_adjtime',
  306 => 'syncfs',
  307 => 'sendmmsg',
  308 => 'setns',
  309 => 'getcpu',
  310 => 'process_vm_readv',
  311 => 'process_vm_writev',
  312 => 'kcmp',
  313 => 'finit_module',
  314 => 'sched_setattr',
  315 => 'sched_getattr',
  316 => 'renameat2',
  317 => 'seccomp',
  318 => 'getrandom',
  319 => 'memfd_create',
  320 => 'kexec_file_load',
  321 => 'bpf',
  322 => 'execveat',
  323 => 'userfaultfd',
  324 => 'membarrier',
  325 => 'mlock2',
  326 => 'copy_file_range',
  327 => 'preadv2',
  328 => 'pwritev2',
  329 => 'pkey_mprotect',
  330 => 'pkey_alloc',
  331 => 'pkey_free',
  332 => 'statx',
  334 => 'rseq',
  424 => 'pidfd_send_signal',
  425 => 'io_uring_setup',
  426 => 'io_uring_enter',
  427 => 'io_uring_register',
  428 => 'open_tree',
  429 => 'move_mount',
  430 => 'fsopen',
  431 => 'fsconfig',
  432 => 'fsmount',
  433 => 'fspick',
  434 => 'pidfd_open',
  435 => 'clone3',
  436 => 'close_range',
  437 => 'openat2',
  438 => 'pidfd_getfd',
  439 => 'faccessat2',
  440 => 'process_madvise',
  441 => 'epoll_pwait2',
  442 => 'mount_setattr',
  443 => 'quotactl_fd',
  444 => 'landlock_create_ruleset',
  445 => 'landlock_add_rule',
  446 => 'landlock_restrict_self',
  447 => 'memfd_secret',
  448 => 'process_mrelease',
  449 => 'futex_waitv',
  450 => 'set_mempolicy_home_node'
}.map { |num, name| [num, "#{num} (#{name})"] }.to_h

REGS_R64 = %w[
  rax
  rbx
  rcx
  rdx
  rsi
  rdi
  rsp
  rbp
  r8
  r9
  r10
  r11
  r12
  r13
  r14
  r15
]

REGS_R32 = %w[
  eax
  ebx
  ecx
  edx
  esi
  edi
  esp
  ebp
  r8d
  r9d
  r10d
  r11d
  r12d
  r13d
  r14d
  r15d
]

REGS_R16 = %w[
  ax
  bx
  cx
  dx
  si
  di
  sp
  bp
  r8w
  r9w
  r10w
  r11w
  r12w
  r13w
  r14w
  r15w
]

REGS_R8 = %w[
  al
  bl
  cl
  dl
  sil
  dil
  spl
  bpl
  r8b
  r9b
  r10b
  r11b
  r12b
  r13b
  r14b
  r15b
]

REG_MAP = (REGS_R64.map { |r| [r, r] } + REGS_R32.zip(REGS_R64) + REGS_R16.zip(REGS_R64) + REGS_R8.zip(REGS_R64)).to_h
REGS_R = (REGS_R64 + REGS_R32 + REGS_R16 + REGS_R8).join('|')

if ARGV.empty?
  warn SYNTAX_STRING
  exit 1
end

file_path = ARGV[0]
raise "no such file: #{file_path}" unless File.exist? file_path

only_used_syscalls = false
ARGV[1..].each do |arg|
  if arg == '--only-used-syscalls'
    only_used_syscalls = true
  else
    warn "invalid argument '#{arg}':\n#{SYNTAX_STRING}"
    exit 1
  end
end

puts 'Running objdump...' unless only_used_syscalls
dump = `objdump -wd -j .text -M intel #{file_path.shellescape}`
raise 'objdump failed' unless $?.exitstatus == 0

puts 'Parsing objdump output...' unless only_used_syscalls
current_fn = nil
code_for_fn = {}
fns_with_syscall = Set.new
fns_with_indirect_syscall = Set.new
dump.split("\n").each do |line|
  if line =~ /\A[0-9a-f]+ <(.+?)>:/
    current_fn = Regexp.last_match(1)
    next
  end

  next unless current_fn
  next if %w[syscall __syscall_cp_c].include?(current_fn) # These are for indirect syscalls.

  code = line.strip.split("\t")[2]
  next if [nil, ''].include?(code)

  code_for_fn[current_fn] ||= []
  code_for_fn[current_fn] << code.gsub(/[\t ]+/, ' ')

  fns_with_syscall.add(current_fn) if code == 'syscall'
  fns_with_indirect_syscall.add(current_fn) if code =~ /<(syscall|__syscall_cp)>/
end

unless only_used_syscalls
  puts "Found #{fns_with_syscall.length} functions doing direct syscalls"
  puts "Found #{fns_with_indirect_syscall.length} functions doing indirect syscalls"
end

syscalls_for_fn = {}
not_found_count = 0
(fns_with_syscall + fns_with_indirect_syscall).each do |fn_name|
  syscalls_for_fn[fn_name] ||= []

  if fn_name =~ /_ZN11parking_lot9raw_mutex8RawMutex9lock_slow.+/
    # Hardcode 'SYS_futex' as this function produces a really messy assembly.
    syscalls_for_fn[fn_name] << 202
    next
  end

  code = code_for_fn[fn_name]

  found = false
  regs = {}
  code.each do |inst|
    if inst =~ /mov (#{REGS_R}),(.+)/
      reg = Regexp.last_match(1)
      value = Regexp.last_match(2)
      regs[REG_MAP[reg]] = if value =~ /#{REGS_R}/
                             regs[REG_MAP[value]]
                           elsif value =~ /0x([0-9a-f]+)/
                             Regexp.last_match(1).to_i(16)
                           end
    elsif inst =~ /xor (#{REGS_R}),(#{REGS_R})/
      reg_1 = Regexp.last_match(1)
      reg_2 = Regexp.last_match(1)
      regs[REG_MAP[reg_1]] = 0 if reg_1 == reg_2
    elsif inst =~ /lea (#{REGS_R}),(.+)/
      reg = Regexp.last_match(1)
      value = Regexp.last_match(2)
      regs[REG_MAP[reg]] = ('syscall' if value.strip =~ /\[rip\+0x[a-z0-9]+\]\s*#\s*[0-9a-f]+\s*<syscall>/)
    elsif inst =~ /(call|jmp) (#{REGS_R})/
      reg = Regexp.last_match(2)
      if regs[REG_MAP[reg]] == 'syscall'
        if !regs['rdi'].nil?
          syscalls_for_fn[fn_name] << regs['rdi']
          found = true
        else
          found = false
        end
      end
    elsif inst =~ /(call|jmp) [0-9a-f]+ <(syscall|__syscall_cp)>/
      if !regs['rdi'].nil?
        syscalls_for_fn[fn_name] << regs['rdi']
        found = true
      else
        found = false
      end
    elsif inst == 'syscall'
      if !regs['rax'].nil?
        syscalls_for_fn[fn_name] << regs['rax']
        found = true
      else
        found = false
      end
    end
  end

  next if found

  puts "WARN: Function triggers a syscall but couldn't figure out which one: #{fn_name}"
  puts '  ' + code.join("\n  ")
  puts
  not_found_count += 1
end

puts "WARN: Failed to figure out syscall for #{not_found_count} function(s)" if not_found_count > 0

fns_for_syscall = {}
syscalls_for_fn.each do |fn_name, syscalls|
  syscalls.each do |syscall|
    fns_for_syscall[syscall] ||= []
    fns_for_syscall[syscall] << fn_name
  end
end

if only_used_syscalls
  puts syscalls_for_fn.values.flatten.sort.uniq.map { |sc| SYSCALLS[sc] || sc }.join("\n")
else
  puts 'Functions per syscall:'
  fns_for_syscall.sort_by { |sc, _| sc }.each do |syscall, fn_names|
    fn_names = fn_names.sort.uniq

    puts "    #{SYSCALLS[syscall] || syscall} [#{fn_names.length} functions]"
    fn_names.each do |fn_name|
      puts "        #{fn_name}"
    end
  end

  puts
  puts 'Used syscalls:'
  puts '    ' + syscalls_for_fn.values.flatten.sort.uniq.map { |sc| SYSCALLS[sc] || sc }.join("\n    ")
end
