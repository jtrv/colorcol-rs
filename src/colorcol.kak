declare-option -hidden int colorcol_timestamp 0
declare-option -hidden range-specs colorcol_ranges
declare-option -hidden range-specs colorcol_replace_ranges
declare-option -hidden line-specs colorcol_flags
declare-option -hidden str colorcol_mode append

declare-option bool colorcol_color_full true
declare-option int colorcol_max_flags 3
declare-option str colorcol_flag_str █
declare-option str colorcol_append_str ■

# Background to composite translucent colors over, e.g. 1a1a2e or #1a1a2e.
# Empty (default) => alpha is passed through as an `rgba:` face, which a terminal
# renders identically to the opaque color. Set this to see alpha.
declare-option str colorcol_alpha_bg ''

define-command -hidden colorcol-update-highlighter %{ evaluate-commands %sh{
  case "$kak_opt_colorcol_mode" in
    background|foreground) printf '%s' 'add-highlighter -override window/colorcol ranges colorcol_ranges';;
    append) printf '%s' 'add-highlighter -override window/colorcol replace-ranges colorcol_replace_ranges';;
    flag) printf '%s' 'add-highlighter -override window/colorcol flag-lines default colorcol_flags';;
    *) printf '%s' "echo -debug 'Unknown colorcol mode: $kak_opt_colorcol_mode'";;
  esac
}}

define-command colorcol-refresh -docstring "Recolor the current buffer now" %{ evaluate-commands %sh{
  [ "$kak_opt_colorcol_timestamp" -eq "$kak_timestamp" ] && exit
  printf %s "evaluate-commands -draft -no-hooks %{
    execute-keys '%'
    echo -to-file '$kak_response_fifo' -- %val{selection}
  }" >"$kak_command_fifo"
  colorcol "$kak_opt_colorcol_mode" "$kak_opt_colorcol_max_flags" "$kak_opt_colorcol_flag_str" \
           "$kak_opt_colorcol_append_str" "$kak_opt_colorcol_color_full" "$kak_response_fifo" \
           "$kak_opt_colorcol_alpha_bg"
  }
  set window colorcol_timestamp %val{timestamp}
}

define-command colorcol-mode -params 1 -shell-script-candidates %{
  printf '%s\n%s\n%s\n%s' background foreground append flag
} -docstring "Change colorcol mode (background/foreground/append/flag)" %{
  set window colorcol_mode %arg{1}
  # The new mode writes a different option (ranges vs replace-ranges vs line-specs),
  # so the buffer must be rescanned even though its timestamp did not change.
  set window colorcol_timestamp 0
  colorcol-refresh
  colorcol-update-highlighter
}

define-command colorcol-enable -docstring "Enable colorcol in this window" %{
  colorcol-update-highlighter
  colorcol-refresh
  # WinDisplay, not BufCreate: a window-scoped BufCreate hook never fires, because
  # BufCreate runs in the new buffer's context before any window displays it.
  hook -group colorcol window WinDisplay .* colorcol-refresh
}

define-command colorcol-refresh-continuous -docstring "Recolor as you type" %{
  hook -group colorcol window NormalIdle .* colorcol-refresh
  hook -group colorcol window InsertIdle .* colorcol-refresh
}

define-command colorcol-refresh-on-save -docstring "Recolor on write" %{
  hook -group colorcol window BufWritePost .* colorcol-refresh
}

define-command colorcol-disable -docstring "Disable colorcol in this window" %{
  # `try`: remove-highlighter raises "no such id" if colorcol was never enabled here.
  try %{ remove-highlighter window/colorcol }
  remove-hooks window colorcol
  set window colorcol_timestamp 0
}
