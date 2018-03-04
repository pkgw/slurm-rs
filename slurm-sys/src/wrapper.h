/* Copyright 2017-2018 Peter Williams <peter@newton.cx> and collaborators */
/* Licensed under the MIT License. */

#include <slurm/slurm.h>
#include <slurm/slurmdb.h>

/* Expose some #defines as enumeration values */

enum {
    SLURMRS_NO_VAL = NO_VAL,
};

/* The official API doesn't expose the memory management functions,
 * but we need them: see discussion in the Rust docs. */

extern void *slurm_try_xmalloc(size_t size, const char *file_name, int line, const char *func_name);
extern void slurm_xfree(void **pointer, const char *file_name, int line, const char *func_name);
