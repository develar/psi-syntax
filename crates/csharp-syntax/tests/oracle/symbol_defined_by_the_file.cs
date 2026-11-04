#define B
#undef C
#if A && B && !C
class Yes { }
#elif C
class No { }
#endif
#define C
