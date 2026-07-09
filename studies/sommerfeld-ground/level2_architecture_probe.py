#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Simon Keimer (DC0SK)
#
# PH9-CHK-006 Level 2 ARCHITECTURE PROBE (negative result — records what does NOT work).
#
# Question: can the Sommerfeld surface wave enter fnec's Hallen SOLVE (to correct
# currents/patterns, not just feedpoint Z), cheaply, by reusing the validated Level-1
# E-field reflected dyadic? Two routes tested on a low horizontal dipole (0.05 lambda,
# nec2c GN2 = 67.26 + j52.61; fnec Hallen carries a large reactance offset so compare
# ground-induced DELTAs, not absolute X):
#
#   (1) Born iteration (feed the extra Sommerfeld field back as a distributed Hallen
#       source, re-solve, iterate): DIVERGES. At 0.05 lambda the surface wave is a
#       STRONG coupling, not a small perturbation, so the fixed point is not a
#       contraction (Z oscillates 30 -> 11 -> 2 -> -4 -> 5; current blows up).
#
#   (2) Direct matrix (move the field->RHS correction to the LHS as a matrix Q, solve
#       once): improves feedpoint dR (-36.8 -> -8.0, toward GN2 -11.6) but the CURRENT
#       SHAPE is wrong (grows too fast toward centre) and it is NOT rigorous -- it mixes
#       the E-field correction into the A-domain Hallen operator. It also does not beat
#       Level 1's reaction correction on Z.
#
# CONCLUSION: the cheap perturbative/hybrid routes into fnec's Hallen architecture do
# not reproduce GN2 currents. Level 2 needs a rigorous approach: either a full EFIE MoM
# with the Sommerfeld reflected dyadic in the impedance matrix (a parallel solver path),
# or the reflected VECTOR-POTENTIAL dyadic + DCIM -- with the open question that fnec's
# Hallen eliminates the scalar potential, where the surface wave lives. This is a
# dedicated multi-session solver increment, not a quick win.

import sys, math, cmath
import numpy as np
sys.path.insert(0, '/home/dc0sk/git/fnec-rust/studies/sommerfeld-ground')
from fast_1d_reduction import eproj_1d  # validated 1-D reflected E-field dyadic
from general_dyadic import k0, eta0, epsc, lam

C0=299792458.0; MU0=4e-7*math.pi; ETA0=MU0*C0
F=14.2e6; K=k0
L=10.556; A=0.001; N=21
DL=L/N; HALF=DL/2
H=0.05*lam                       # height
X=np.array([-L/2+(i+0.5)*DL for i in range(N)])  # x positions
FEED=N//2
GAMMA=(cmath.sqrt(epsc)-1)/(cmath.sqrt(epsc)+1)

GL8_N=[-0.960289856497536,-0.796666477413627,-0.525532409916329,-0.183434642495650,
0.183434642495650,0.525532409916329,0.796666477413627,0.960289856497536]
GL8_W=[0.101228536290376,0.222381034453374,0.313706645877887,0.362683783378362,
0.362683783378362,0.313706645877887,0.222381034453374,0.101228536290376]
GL4_N=[-0.861136311594953,-0.339981043584856,0.339981043584856,0.861136311594953]
GL4_W=[0.347854845137454,0.652145154862626,0.652145154862626,0.347854845137454]
def green(r): return cmath.exp(-1j*K*r)/r

def a_free():
    M=np.zeros((N,N),complex)
    for i in range(N):
        for j in range(N):
            if i==j:
                sm=0j
                for xi,wi in zip(GL4_N,GL4_W):
                    l=xi*HALF; r=math.sqrt(l*l+A*A); sm+=wi*(green(r)-1/r)
                sm*=HALF; re=math.sqrt(HALF*HALF+A*A)
                M[i,j]=sm+2*math.log((HALF+re)/A)
            else:
                s=0j
                for xi,wi in zip(GL8_N,GL8_W):
                    xp=X[j]+xi*HALF; r=math.sqrt((X[i]-xp)**2+A*A); s+=wi*green(r)
                M[i,j]=s*HALF
    return M

def a_refl_scalar():
    # scalar-Γ reflected axial A-kernel: Γ·(x̂·(-x̂))·∫G(image) = -Γ·∫G(R_image)
    M=np.zeros((N,N),complex)
    for i in range(N):
        for j in range(N):
            s=0j
            for xi,wi in zip(GL8_N,GL8_W):
                xp=X[j]+xi*HALF
                r=math.sqrt((X[i]-xp)**2+(2*H)**2+A*A); s+=wi*green(r)
            M[i,j]=-GAMMA*s*HALF
    return M

def solve(Amat, b):
    cos_vec=np.cos(K*X)
    M=np.zeros((N+2,N+1),complex); y=np.zeros(N+2,complex)
    M[:N,:N]=Amat; M[:N,N]=-cos_vec; y[:N]=b
    M[N,0]=1.0; M[N+1,N-1]=1.0
    x=np.linalg.lstsq(M,y,rcond=None)[0]
    return x[:N]

def rhs_delta():
    scale=2*math.pi/ETA0
    return np.array([-1j*scale*math.sin(K*abs(X[i]-X[FEED])) for i in range(N)],complex)

def rhs_from_field(Ex):
    # Hallén RHS from an incident axial field Ex(x): b(x)=(j 2π/η)∫ sin(k|x-x'|) Ex(x') dx'
    scale=2*math.pi/ETA0
    b=np.zeros(N,complex)
    for i in range(N):
        s=0j
        for jp in range(N):
            s+=math.sin(K*abs(X[i]-X[jp]))*Ex[jp]*DL
        b[i]=1j*scale*s
    return b

# --- baseline solves ---
Afree=a_free(); Arefl=a_refl_scalar()
b0=rhs_delta()
I_fs=solve(Afree,b0);     Zfs=1/I_fs[FEED]
I_rcm=solve(Afree+Arefl,b0); Zrcm=1/I_rcm[FEED]
print(f"FREE : Z={Zfs.real:.2f}{Zfs.imag:+.2f}j   (nec2c 78.85+j44.70)")
print(f"RCM  : Z={Zrcm.real:.2f}{Zrcm.imag:+.2f}j   (nec2c GN0 46.47+j63.59)")

# --- Born iteration: add Sommerfeld surface-wave field, re-solve ---
def dE_from_current(I):
    # extra Sommerfeld field on each segment i: sum_j [E_Somm(j->i)-Γ E_pec(j->i)] I_j dl
    dE=np.zeros(N,complex)
    xhat=[1.0,0,0]
    for i in range(N):
        s=0j
        for j in range(N):
            dX=X[i]-X[j]; d=2*H
            es=eproj_1d(xhat,xhat,dX,0.0,d,pec=False)
            ep=eproj_1d(xhat,xhat,dX,0.0,d,pec=True)
            s+=(es-GAMMA*ep)*I[j]*DL
        dE[i]=s
    return dE

# --- DIRECT solve: build the surface-wave correction as a matrix Q, solve once ---
xhat=[1.0,0,0]
dEop=np.zeros((N,N),complex)   # field on i from unit current moment on j
for i in range(N):
    for j in range(N):
        es=eproj_1d(xhat,xhat,X[i]-X[j],0.0,2*H,pec=False)
        ep=eproj_1d(xhat,xhat,X[i]-X[j],0.0,2*H,pec=True)
        dEop[i,j]=(es-GAMMA*ep)*DL
scale=2*math.pi/ETA0
RHSop=np.zeros((N,N),complex)  # field -> Hallén RHS
for m in range(N):
    for p in range(N):
        RHSop[m,p]=1j*scale*math.sin(K*abs(X[m]-X[p]))*DL
Q=RHSop@dEop
for sgn in (+1,-1):
    Idir=solve(Afree+Arefl+sgn*Q, b0)
    Z=1/Idir[FEED]
    print(f"DIRECT (Q sign {sgn:+d}): Z={Z.real:.2f}{Z.imag:+.2f}j   (nec2c GN2 67.26+j52.61)")
    if sgn==+1: Ikeep=Idir
print("nec2c GN2 current mag: 1.06e-3 2.95e-3 4.68e-3 6.28e-3 7.72e-3 8.97e-3 ...")
print("direct  current mag :", " ".join(f"{abs(x):.2e}" for x in Ikeep[:6]),"...")
