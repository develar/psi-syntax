
#if false
    #define A
#elif false
    #define B
#else
    #if A
 ?1 // no errors here
    #endif
    #if B
 ?2 // no errors here
    #endif
#endif
