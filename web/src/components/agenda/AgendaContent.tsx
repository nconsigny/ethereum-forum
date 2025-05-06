import classNames from 'classnames';
import { FC, useState } from 'react';
import { LuCalendar } from 'react-icons/lu';

import { useEventsRecent, useEventsUpcoming } from '@/api/events';

import { Meetings } from './Meetings';

export const AgendaContent: FC = () => {
    const [tab, setTab] = useState<'upcoming' | 'recent'>('upcoming');
    const { data: upcoming } = useEventsUpcoming();
    const { data: recent } = useEventsRecent();

    return (
        <>
            <div className="card">
                <div className="flex justify-between">
                    <div>
                        <h2 className="text-lg font-bold">Protocol Agenda</h2>
                        <p>
                            the protocol agenda is a shared calendar that includes all meetings and
                            live streams around the protocol.
                        </p>
                        <p>
                            See also{' '}
                            <a
                                href="https://github.com/ethereum/pm/issues"
                                target="_blank"
                                rel="noreferrer"
                                className="link"
                            >
                                github.com/ethereum/pm/issues
                            </a>
                        </p>
                    </div>
                    <div>
                        <a
                            href="https://calendar.google.com/calendar/u/0?cid=Y191cGFvZm9uZzhtZ3JtcmtlZ243aWM3aGs1c0Bncm91cC5jYWxlbmRhci5nb29nbGUuY29t"
                            target="_blank"
                            className="button px-1 flex items-center gap-1"
                            rel="noreferrer"
                        >
                            <LuCalendar />
                            <span>Add to calendar</span>
                        </a>
                    </div>
                </div>
            </div>
            <div className="flex gap-1">
                <button
                    className={classNames('tab button', tab === 'upcoming' && 'active')}
                    onClick={() => setTab('upcoming')}
                >
                    Upcoming
                </button>
                <button
                    className={classNames('tab button', tab === 'recent' && 'active')}
                    onClick={() => setTab('recent')}
                >
                    Recent
                </button>
            </div>
            {tab === 'upcoming' ? (
                <Meetings data={upcoming ?? []} key="upcoming" />
            ) : (
                <Meetings data={recent ?? []} key="recent" />
            )}
        </>
    );
};
