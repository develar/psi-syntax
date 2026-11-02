#define error
#if error
#warning W1
#endif

#undef error
#if error
#warning W2
#endif
